// power_idle.h — screen-off and sleep: the idle screen-off timer, darkening the panel, the SoC's
// suspend to RAM, the framebuffer early-suspend handshake, and the USB port after a suspend.
//
// Moved out of main.cpp (2026-11-24). Not self-contained: included once, inside main.cpp's
// anonymous namespace, after the backlight, boot-to-stock, noise-cancelling and Wi-Fi blocks.
#pragma once

// ── Idle screen-off (opt-in, Settings > Screen-off timer) ─────────────────────────────────────
// Blanks the BACKLIGHT after N seconds with no input, to save power (goal #1). Two rules make this
// safe enough to ship without a device to test on:
//
//  1. It is OFF by default. Nothing changes unless the user picks a duration, so the worst case is
//     opt-in rather than inflicted on every boot.
//  2. The auto-off path does NOT sleep the touch controller, unlike the Power-button path. A
//     sleeping controller reports nothing, so wake-on-touch would be impossible and a dark panel
//     would look like a dead device. Keeping it awake costs a little current; the backlight is the
//     dominant draw anyway.
//
// And the escape ladder still holds: even if wake-on-touch fails entirely, the physical Power
// button restores the screen. That escape depends on strictly less than the thing it rescues (a
// key event vs. the whole touch stack).
static const int  kWifiIdleOffS = 2 * 60;  // Wi-Fi sleeps after this long dark and unused
static const int  kWifiServeIdleS = 10 * 60; // ...or this long, and this long unused, with a server on
static const int  kBtIdleOffS = 10 * 60;   // "Turn off when idle": this long dark and silent
static const long kIdleLockMs = 5 * 60 * 1000L;   // an idle blank locks (touch asleep) after this
static bool g_screen_auto_off = false;   // dark because of the idle timer (not the Power button)
static bool g_held = false;              // Hold/lock switch engaged (mirrors cinder_set_hold)

// Adopt the Hold switch's ACTUAL position, rather than waiting for it to move.
//
// evdev delivers state CHANGES. The only place cinder_set_hold() was ever called is the event
// branch in input_pump, so a device booted with Hold ALREADY engaged came up UNLOCKED — and stayed
// that way until the switch was flicked twice, with the UI's idea of the lock INVERTED relative to
// the physical switch in between. Reported 2026-08-16. A pocket-safe device that boots unlocked
// when the switch says locked is the wrong way round for this bug to fail.
//
// Asserts BOTH ways on purpose. This also runs on any path that reopens the nodes, where the switch
// may have been released while we were not listening — so "no hold code is down" has to actively
// clear the state, not merely decline to set it.
//
// It is also the groundwork for a planned escape: a quick Hold ON→OFF flick as the failsafe out of
// a fully-dark backlight (see the backlight-off work). That escape is only trustworthy if the lock
// state can be read rather than inferred, which is what this gives it.
static void sync_hold_from_hw() {
    // WHICH codes mean Hold is a question for the LIVE keymap, not for a hardcoded 35:
    // cinder_keymap.conf can move it, and a device whose conf remaps the switch must still come up
    // locked. Collect them first so each node is only walked once.
    int codes[16];
    int ncodes = 0;
    for (int c = 0; c < keymap_size() && ncodes < (int)(sizeof codes / sizeof *codes); ++c)
        if (g_keymap[c] == CINDER_BTN_HOLD) codes[ncodes++] = c;
    if (ncodes == 0) return;   // nothing is mapped to Hold — nothing to read

    // One bit per code, so the buffer must cover the highest code we will test (keymap_size()).
    unsigned char bits[(768 + 7) / 8];
    bool held = false;
    int on_node = -1, on_code = -1;
    for (int i = 0; i < g_evn && !held; ++i) {
        std::memset(bits, 0, sizeof bits);
        if (ioctl(g_evfds[i], eviocgkey(sizeof bits), bits) < 0) continue;   // node carries no keys
        for (int k = 0; k < ncodes; ++k) {
            const int c = codes[k];
            if (c < 0 || c >= (int)(sizeof bits) * 8) continue;
            if (bits[c / 8] & (1u << (c % 8))) { held = true; on_node = i; on_code = c; break; }
        }
    }

    // Only speak when it changes something. At boot g_held is already false, so the ordinary case
    // (switch off) writes nothing and logs nothing.
    if (held == g_held) return;
    g_held = held;
    cinder_set_hold(held ? 1 : 0);
    char m[128];
    if (held)
        std::snprintf(m, sizeof m,
                      "input: Hold switch is ALREADY ENGAGED (node %d, code %d) — locking",
                      on_node, on_code);
    else
        std::snprintf(m, sizeof m, "input: Hold switch reads released — unlocking");
    clog_(m);
}
long long g_last_input_ms = 0;                // when we last saw ANY input event (see the fwd decl above)
static long long g_peer_pause_ms = -1000000;  // the headphones' last PAUSE/STOP (AVRCP 201/166)

// ── Actually darken the panel ───────────────────────────────────────────────────────────────────
//
// THE SYSFS NODE IS NOT ENOUGH, and this file has recorded that since 2026-08-19 without acting on
// it. `set_backlight`'s own comment states the measurement: node at 0, DisplayService at 2, panel
// STILL LIT — "backlight off still powers the backlight". `set_backlight` handles it (level 0 →
// `display_backlight(0)`), but the two paths that blank the screen never went through
// `set_backlight`: both `screen_auto_off` (idle timeout) and `screen_toggle`'s off-branch did a
// raw `fputc('0')` to the node and stopped there. So the idle timer and the Power button each
// "turned the screen off" by writing a number that the service overrides, and the panel stayed on.
//
// That is a standing bug in its own right, and NOT the `g_ipc_dead` story: it does not need a
// guard latch, a crash or anything else to have gone wrong — it is simply the service half of the
// write missing from both callers. (It was briefly attributed to the latch while diagnosing the
// 2026-09-02 wedge; that was wrong, and this is the actual mechanism.)
//
// `display_backlight_remember()` FIRST, because it is what makes the restore possible: it caches
// the service's own level while that level is still non-zero, and `set_backlight` reads it back
// through `g_disp_bl_saved` when the panel comes on again. Zeroing before remembering would cache
// nothing and wake the device to a dark screen.
//
// Unguarded, exactly like every other `display_backlight` call site (`set_backlight` runs on each
// theme toggle and brightness step). Wrapping only this one in `run_guarded` would make a display
// hang cost Sony IPC for the boot — a strictly worse trade than the blank not landing.
// DO NOT WRITE /sys/power/state FROM HERE. There was a `soc_early_suspend()` helper here that did,
// and it cost a forced reboot on 2026-09-04. The record, so nobody rebuilds it:
//
// THE PRIZE WAS REAL. Cinder's screen-off is cosmetic as far as the SoC is concerned — it writes
// the backlight node and calls Sony's display_backlight(0), and the kernel never learns the device
// went idle. The MTK early-suspend chain, which every driver registers its low-power handler with,
// is started only by a write to /sys/power/state and nothing on this system writes it. Measured:
// the SoC has entered deep idle ZERO times in every boot ever sampled, blocked on one byte that
// only that chain sets. See analysis/RE_early_suspend.md for the disassembly.
//
// WHY IT CANNOT BE DONE THIS WAY. The chain does not stop at early suspend: it ends with
// `pm_autosleep_set_state(3)`, which is the real suspend-to-RAM path. ON THE CABLE that never
// fires, because USB holds a wakeup source — which is why four on-cable tests all looked clean and
// resumed perfectly. OFF THE CABLE AND IDLE, nothing holds one: the device suspended to RAM and
// **neither the Power key nor plugging USB in would wake it.** It took a forced reboot.
//
// The reassurance that made this look safe was that Sony's codec driver takes a `CXD3778GF` wakeup
// source during playback (measured, and true). That protects the PLAYING case and says nothing at
// all about the idle case — which is the only case this code ever ran in. Testing exclusively on
// the cable hid the entire failure mode.
//
// WHAT WOULD HAVE TO BE TRUE FIRST: a wake source proven to bring the device back from
// suspend-to-RAM off-cable. Until someone has demonstrated that — with a way back in that does not
// depend on the thing being suspended — this stays unwired. An escape must depend on less than
// what it rescues, and here there was no escape at all.
//
// ─── THAT CONDITION WAS MET ON 2026-09-04, AND THE ABOVE IS NOW HISTORY ───────────────────────
//
// The device suspends and resumes correctly off-cable. Measured, repeatedly, with the harness in
// `tools/pmtest.sh`: `icx_pm_helper/resume_count` went 0→5 in one run and 0→3 in another, and
// `dpidle_cnt[0]` went **0 → 78** — the SoC entered deep idle for the first time, which is the
// whole point of this exercise. Logs in `analysis/kernel/`.
//
// The original failure was never a failure to resume. It was three ordinary things at once:
//   1. suspend/resume working fine, but
//   2. NOTHING writing `on` back to /sys/power/state, so the device never left the suspend state —
//      it just cycled (suspend, wake on a timer, ~20 s awake on Sony's resume wakelock, suspend
//      again) with the panel dark the whole time, and
//   3. the USB gadget not re-enumerating after a resume, so a replug found nothing.
// From outside, a device cycling perfectly is indistinguishable from a hung one. Both earlier
// diagnoses ("no wake source", then "the resume path is broken") were wrong.
//
// So the exit is the load-bearing part, and it is what the code below exists to do.
static void panel_dark() {
    if (!g_bl_read) load_bl_cfg();
    display_backlight_remember();   // cache the service level BEFORE zeroing it
    if (g_bl.valid) {
        FILE* f = std::fopen(g_bl.path, "w");
        if (f) { std::fputc('0', f); std::fclose(f); }
    }
    display_backlight(0);           // the half that actually darkens the glass
}

// Blank the panel WITHOUT sleeping the touch controller, so a touch can wake it.
static void screen_auto_off() {
    if (!g_screen_on) return;
    g_screen_on = false;
    g_screen_auto_off = true;
    // Drop any in-flight contact so the waking touch starts a fresh gesture.
    g_touch_down = false; g_touch_start_x = -1; g_touch_start_y = -1; g_touch_saw_pos = false;
    g_drag_active = false; g_drag_vel = 0.0f;
    g_scrub_active = false; g_scrub_tested = false; g_hswipe_active = false; g_reorder_active = false; g_sbar_active = false;
    g_shelfswipe_active = false; g_shelfswipe_tested = false;
    g_quickpull_active = false; g_quickpull_tested = false;
                            g_touch_down_ms = now_ms(); g_reorder_hold_tested = false; g_hold_queued = false;
    panel_dark();
    apply_pump_interval();   // nothing on screen needs 50 Hz IPC latency
    clog_("screen: idle timeout -> panel off (touch or Power wakes it)");
}

static void soc_suspend_leave_now(const char* why);   // defined with socsusp, below

// Wake from an idle blank. No-op unless WE turned it off: a Power-button blank must stay off until
// Power is pressed again (that is the pocket-safe case, and the Hold switch's job otherwise).
static void screen_auto_wake() {
    if (!g_screen_auto_off) return;
    g_screen_auto_off = false;
    g_screen_on = true;
    soc_suspend_leave_now("screen wake");   // first: the LCM needs late resume before it can light
    // Self-heal: the idle blank never sleeps the touch panel (it is what wakes us), but a Power
    // blank does now — and if the user Power-blanked, woke with Power, and the panel is dark again
    // by the idle timer, re-validating here costs one call and closes the only window where a
    // stuck "invalid" could survive.
    touch_set_sleep(0);
    apply_backlight();
    cinder_force_dirty();   // the render loop skipped painting while dark — repaint immediately
    apply_pump_interval();
    clog_("screen: woken by input");
}

void screen_toggle() {
    g_screen_auto_off = false;   // an explicit Power press takes ownership of the panel state
    g_screen_on = !g_screen_on;
    // Leave stage 1 FIRST, before the touch IPC: with the controller early-suspended that call took
    // 270 ms on device (2026-09-28), all of it spent in front of a black screen.
    if (g_screen_on) soc_suspend_leave_now("Power");
    touch_set_sleep(g_screen_on ? 0 : 1);   // stock behaviour: TS sleeps with the panel (battery)
    // Drop any in-flight contact: the sleeping controller never sends its lift, and a stale
    // "down" would make the next touch classify as a drag from the old start point.
    g_touch_down = false; g_touch_start_x = -1; g_touch_start_y = -1; g_touch_saw_pos = false;
    apply_pump_interval();
    if (g_screen_on) {
        apply_backlight();
        cinder_force_dirty();
        // Waking is the other moment the mixer can have drifted from the level the UI is about to
        // show — the panel comes back with a number on it, and that number had better be true.
        // Verify-first, so this costs one read when nothing has changed.
        bt_resync_volume("screen wake");
        return;
    }
    panel_dark();
}

// ── SoC suspend-to-RAM ────────────────────────────────────────────────────────────────────────
//
// Turning the panel off is cosmetic to the kernel; only a write to /sys/power/state starts the
// early-suspend chain, and only that lets the SoC reach deep idle. See the long history note
// above `panel_dark()` for why this took three attempts and two forced reboots to get right.
//
// The rules this obeys, each one bought with a failure:
//
//   * NEVER while playing. `g_playing` gates every path. Screen-off during playback must not
//     touch the audio — that is a standing requirement, not a nicety.
//   * NEVER without an escape. `/contents/cinder_no_suspend` disables the whole thing, and
//     /contents is the vfat volume a PC can mount over USB-MSC — so a device that will not stay
//     awake is still rescuable without a flash. An escape must depend on less than what it rescues.
//   * NEVER in the first minutes of a boot. `kBootGraceS` guarantees a window in which the device
//     is reachable no matter what this code does.
//   * ALWAYS write the exit. A resume that nobody acknowledges leaves the device cycling with a
//     dark screen, which is exactly what looked like a brick.
//
// STAGE 1 IS ON BY DEFAULT since 2026-09-28 (5 s of screen-off idle — 60 until 10-04, and screen-off playback on
// the jack); stage 2 is still opt-in. /contents/cinder_suspend_s overrides the threshold (0 = off)
// and /contents/cinder_no_suspend stops everything — files rather than a settings row because they
// stay reachable from a PC over USB-MSC when the app is the thing misbehaving. See threshold_s().
// ── FRAMEBUFFER EARLY-SUSPEND HANDSHAKE ────────────────────────────────────────────────────────
// Android's fb early-suspend handler asks userspace to stop drawing and WAITS one second for an
// answer. Nothing here ever answered, so every single early suspend paid the full timeout:
//
//   stop_drawing_early_suspend: timeout waiting for userspace to stop drawing
//
// Measured 2026-09-04: entering early suspend took 1.31 s wall (257.959 -> 259.268 in dmesg) and
// 1.00 s of that was this one handler timing out. Answering cuts entry latency by ~75%.
//
// PROTOCOL, read out of the kernel image rather than assumed (fb_state @0xc0c37bc0, states named
// by what the code does with them, not by AOSP convention):
//
//   fb_state == 2  DRAWING             set by start_drawing_late_resume  (c00be61c)
//   fb_state == 1  REQUEST_STOP        set by stop_drawing_early_suspend (c00be8b0), which then
//                                      waits for fb_state == 0 with a 1 s timeout
//   fb_state == 0  STOPPED_DRAWING     set by *wait_for_fb_wake_show*    (c00be76c)
//
// The acknowledgement is in **wait_for_fb_wake, not wait_for_fb_sleep** — that is the whole trick,
// and it is the opposite of what the node names suggest. Reading wait_for_fb_wake is what takes
// fb_state 1 -> 0 and wakes the queue the early-suspend handler is sleeping on. So the loop must
// read BOTH nodes, in order, and must not dawdle between them or the timeout fires anyway:
//
//   read(wait_for_fb_sleep)  blocks while fb_state == 2, returns "sleeping"  -> suspend beginning
//   read(wait_for_fb_wake)   ACKs (1 -> 0), then blocks until fb_state == 2, returns "awake"
//
// Both reads return -ERESTARTSYS if a signal is pending, so EINTR is normal and must be retried.
//
// This needs a dedicated thread because both reads block indefinitely, and it must not be the
// thread that owns SIGALRM — that is the render worker's (run_guarded's alarm is process-directed;
// see the guard notes above), so SIGALRM is blocked here.
//
// Safe because by the time stage 1 fires, Cinder has not painted for minutes: the panel blanks on
// its own idle timer long before the suspend threshold, so "stop drawing" is already true and the
// ACK only tells the kernel something it could not otherwise find out.
namespace fbsync {

const char kSleepNode[] = "/sys/power/wait_for_fb_sleep";
const char kWakeNode[]  = "/sys/power/wait_for_fb_wake";

// Best-effort view of the panel, published for whoever wants it. `g_fb_last_wake_ms` is a real
// late-resume stamp straight from the kernel — strictly better than noticing a resume_count change
// on the next 1 Hz tick, which is how socsusp currently finds out.
volatile int  g_fb_awake      = 1;
volatile long long g_fb_last_wake_ms = 0;

// One open+read per call: these are sysfs show() handlers, so the blocking wait happens inside the
// read and a fresh open is needed each time round.
int read_blocking(const char* path, char* buf, size_t n) {
    for (;;) {
        const int fd = ::open(path, O_RDONLY);
        if (fd < 0) return -1;
        const ssize_t r = ::read(fd, buf, n - 1);
        const int e = errno;
        ::close(fd);
        if (r >= 0) { buf[r] = 0; return (int)r; }
        if (e == EINTR) continue;
        return -1;
    }
}

void* thread(void*) {
    sigset_t sa; sigemptyset(&sa); sigaddset(&sa, SIGALRM);
    pthread_sigmask(SIG_BLOCK, &sa, nullptr);

    char buf[32];
    int  cycles = 0;
    int  fast   = 0;
    for (;;) {
        const long long t0 = now_ms();
        if (read_blocking(kSleepNode, buf, sizeof buf) < 0) {
            clog_("fbsync: wait_for_fb_sleep unreadable — handshake off for this boot");
            return nullptr;
        }
        g_fb_awake = 0;

        if (read_blocking(kWakeNode, buf, sizeof buf) < 0) {
            clog_("fbsync: wait_for_fb_wake unreadable — handshake off for this boot");
            return nullptr;
        }
        g_fb_awake = 1;
        g_fb_last_wake_ms = now_ms();

        // A node that answers instantly instead of blocking would turn this into a busy loop that
        // eats a core for the life of the process. Neither read is supposed to return quickly —
        // one waits for suspend, the other for resume — so a fast round trip means the contract is
        // not what the disassembly said, and the right response is to stop rather than to spin.
        if (now_ms() - t0 < 50) {
            if (++fast >= 20) {
                clog_("fbsync: nodes are not blocking (20 instant cycles) — handshake off, this is a busy loop");
                return nullptr;
            }
        } else {
            fast = 0;
        }

        // Log the first few, then go quiet: this fires on every screen-off for the life of the
        // process, and the interesting part is that it works at all.
        if (++cycles <= 3) {
            char m[96];
            std::snprintf(m, sizeof m, "fbsync: fb sleep/wake cycle %d acknowledged (no 1 s stall)", cycles);
            clog_(m);
        }
    }
}

void start() {
    if (adv_on("no_fbsync")) { clog_("fbsync: disabled (/contents/cinder_no_fbsync)"); return; }
    if (::access(kSleepNode, R_OK) != 0 || ::access(kWakeNode, R_OK) != 0) {
        clog_("fbsync: /sys/power/wait_for_fb_* absent — nothing to answer");
        return;
    }
    pthread_attr_t at;
    pthread_attr_init(&at);
    pthread_attr_setdetachstate(&at, PTHREAD_CREATE_DETACHED);
    pthread_t th;
    if (pthread_create(&th, &at, thread, nullptr) != 0) clog_("fbsync: pthread_create FAILED");
    else clog_("fbsync: answering the fb early-suspend handshake (saves ~1 s per suspend)");
    pthread_attr_destroy(&at);
}

} // namespace fbsync

bool contents_mounted();   // defined with the USB-MSC block, far below

namespace socsusp {

const char kConfigFile[]  = "/contents/cinder_suspend_s";
const char kStateNode[]   = "/sys/power/state";
const char kWakeLock[]    = "/sys/power/wake_lock";
const char kWakeUnlock[]  = "/sys/power/wake_unlock";
bool g_ram_released = false;   // stage 2: our wakelock is released and the kernel may be suspending
// Where the resume counter stood, and when (monotonic, so suspended time does not count), as the
// wakelock went. The loop watches the counter every pass for kResumeWatchMs instead of once a
// second, so a key that wakes the player is acted on in ~0.1 s, not up to a second later.
long      g_ram_rc = -1;
long long g_ram_at_ms = 0;
const long long kResumeWatchMs = 5000;
const char kResumeCount[] = "/sys/devices/platform/icx_pm_helper/resume_count";
const char kSpmR12[]      = "/sys/devices/platform/icx_pm_helper/spm_r12";
const char kLockName[]    = "cinder";

// TWO STAGES, AND ONLY THE SECOND ONE IS RISKY. This is the shape Sony's own init file implies:
// /init.hagoromo.rc does `chmod 0666` on /sys/power/state, wake_lock, wake_unlock AND
// icx_pm_helper/resume_lock_cancel, i.e. the vendor intended a userspace power manager driving
// suspend through WAKELOCKS rather than through bare state writes.
//
//   Stage 1 — EARLY SUSPEND, wakelock HELD.  Writing `mem` to /sys/power/state runs the
//     early-suspend chain, which is what sets the flag the SoC's deep-idle gate reads. Holding a
//     wakelock at the same time stops autosleep from taking the next step into suspend-to-RAM. The
//     result is a device that reaches DEEP IDLE between timer ticks while staying fully alive:
//     USB works, adb works, and there is nothing to recover from. Measured: `dpidle_cnt` climbs
//     during exactly these windows.
//   Stage 2 — SUSPEND TO RAM, wakelock RELEASED.  More saving, because the CPU is off entirely.
//     But the USB gadget does not come back afterwards: measured 2026-09-04, after a resume the
//     musb OTG state machine sits in `a_idle` instead of `b_peripheral` and the host never sees
//     the device again, despite VBUS and charger-detect both being fine. Until that is fixed a
//     suspend costs a reboot to undo. Stage 2 was opt-in until 2026-10-24; now `no_ram_suspend` turns it off.
//
// Note stage 1 buys nothing while plugged in: the cable holds the USB0 clock on, and that alone
// blocks deep idle (`dpidle_block_mask[CG_PERI0]=0x400`). It is an off-cable win, which is the
// case that matters for battery anyway.
//
// DO NOT try to tell a button wake from a timer wake using SPM's R12. That was tried and it is
// wrong on this SoC: a Power press measured 2026-09-04 woke the device after 17 s with the wake
// timer set to 300 s — unambiguously the button — and R12 still read 0x20, which is GPT. `eint_sta`
// is no better: bit 22 of word 4 (EINT 150, the PMIC line) reads set on runs with no button press
// at all, so it is a mask or a latch rather than a per-wake record.
//
// So this does not classify the wake. On ANY resume it simply writes `on` to leave the suspend
// state cleanly, WITHOUT lighting the panel, and lets the ordinary idle countdown start again. If
// the user did press Power, Cinder's normal input path lights the screen on its own — no special
// case needed. If a timer woke it, the device idles back down after the threshold.
//
// This is strictly better than classifying: it cannot mis-classify, it always leaves the kernel in
// a known state, and the failure mode of a spurious wake is one extra idle countdown rather than a
// device stuck cycling with a dark screen.

const int kRamDwellS  = 10;    // stage 1 this long before stage 2: stock's AutoSleepTimer default (was 60)
const int kBootGraceS = 180;   // never suspend in the first 3 minutes of an app start

bool write_node(const char* path, const char* val) {
    FILE* f = std::fopen(path, "w");
    if (!f) return false;
    const bool ok = std::fputs(val, f) >= 0;
    std::fclose(f);
    return ok;
}

long read_node_long(const char* path) {
    FILE* f = std::fopen(path, "r");
    if (!f) return -1;
    char buf[64] = {0};
    const bool got = std::fgets(buf, sizeof buf, f) != nullptr;
    std::fclose(f);
    if (!got) return -1;
    return (long)std::strtol(buf, nullptr, 0);
}

bool file_exists(const char* path) {
    FILE* f = std::fopen(path, "r");
    if (!f) return false;
    std::fclose(f);
    return true;
}

// Idle seconds before suspending, or 0 for disabled. Latched once decided: deliberately not
// hot-reloadable, because a half-written config file should not be able to change it mid-boot.
//
// ON BY DEFAULT since 2026-09-28 (stage 2 since 2026-10-24, `no_ram_suspend` to turn it off). Absent file =
// kDefaultThresholdS. A file holding 0 turns it off; a positive number is the threshold; anything
// unparseable falls back to the default and says so. The escape hatch (`no_suspend`) is still
// checked every tick. What earned the default: stage 1 holds a wakelock, so it can never reach the
// suspend-to-RAM path that cost the 2026-09-04 forced reboot; it ran clean off-cable on 09-04
// (486 s idle, 18,509 deep-idle entries, USB back on replug) and 09-06 (205 s playing the jack),
// and on Walkman One on 09-28. Off, the SoC had entered deep idle zero times in every boot sampled.
//
// BUT A FAILED READ IS NOT AN ANSWER. /contents is a vfat partition the USB mass-storage gadget
// also binds as a LUN, and it does so DURING BOOT: `fsg_store_file file=/emmc@contents` at 10.1,
// 11.1 and 12.7 s (dmesg, 2026-09-04) — cinder-home itself starts at 10.17 s. A boot that looked
// one tick too early would see "no file" on an unmounted mountpoint and ignore a user's explicit 0,
// so nothing is decided until /contents is really mounted. Undecided costs one check a second and
// fails in the safe direction (no suspend) — and the boot grace is longer than the window anyway.
//
// Either way the decision is LOGGED, because the failure that matters is the one leaving no trace.
//
// 5 s SINCE 2026-10-04 (was 60; the owner's choice the same day, after 15 was tried). Until stage 1
// the panel is dark and nothing else is: VENCPLL, the display power domain and 13 display clocks
// all stay up (/proc/clkmgr, same day), so every screen-off paid 60 s of a scanning display. What
// the wait buys is a faster wake — Power to backlight is 0.23 s from a dark panel and 0.48 s from
// stage 1 (log timestamps, Walkman One) — and 5 s keeps that only for a press straight after the
// blank. Entering and leaving stage 1 mid-song is inaudible: recorded off the jack across both, no
// gap of 3 ms or more.
const int kDefaultThresholdS = 5;

int threshold_s() {
    static int cached  = -1;
    static int retries = 0;
    if (cached >= 0) return cached;

    if (!contents_mounted()) { ++retries; return 0; }   // undecided: ask again next tick

    char m[144];
    char adv[32];
    if (!file_exists(kConfigFile) && adv_value("suspend_s", adv, sizeof adv)) {
        char* end = nullptr;
        const long v = std::strtol(adv, &end, 10);
        cached = (end != adv && v >= 0 && v < 100000) ? (int)v : kDefaultThresholdS;
        std::snprintf(m, sizeof m, "suspend: idle threshold %d s (suspend_s=%s in cinder_advanced.conf; 0 = off)",
                      cached, adv);
        clog_(m);
        return cached;
    }
    if (!file_exists(kConfigFile)) {
        cached = kDefaultThresholdS;
        std::snprintf(m, sizeof m,
                      "suspend: enabled by default, idle threshold %d s (0 in /contents/cinder_suspend_s "
                      "turns it off; %d retries)", cached, retries);
        clog_(m);
        return cached;
    }
    const long v = read_node_long(kConfigFile);
    if (v == 0) {
        cached = 0;
        clog_("suspend: disabled by /contents/cinder_suspend_s = 0");
    } else if (v > 0 && v < 100000) {
        cached = (int)v;
        std::snprintf(m, sizeof m, "suspend: enabled, idle threshold %d s (/contents/cinder_suspend_s)", cached);
        clog_(m);
    } else {
        cached = kDefaultThresholdS;
        std::snprintf(m, sizeof m,
                      "suspend: /contents/cinder_suspend_s is unreadable or out of range — using the "
                      "default %d s", cached);
        clog_(m);
    }
    return cached;
}

// Stage 1 active: `mem` written, wakelock held. Namespace-level (not a static in the tick) so the
// wake path can leave at once instead of waiting up to a second for the next tick.
bool g_early = false;

// RESUME AS A USB DEVICE, NOT A HOST (2026-10-05, analysis/RE_usb_after_resume.md). The kernel's
// `musb_bus_resume` switches the port to host on every resume unless the board ID `icx_bid2` is 3.
// As a host it is powered with nobody attached, so the driver ignores a cable plugged in later and
// USB stays dead until a restart (checklist 33.2). So: 3 while suspended, the player's own value
// back within a second of waking. Nothing else reads the ID in that window (cinder-power.c).
const char kBid2[] = "/sys/module/icx_pm_helper/parameters/icx_bid2";
long g_bid2_own = -1;   // the value to put back; -1 = not swapped
void resume_as_device(bool on) {
    char cmd[80];
    if (on) {
        const long own = read_node_long(kBid2);
        if (own < 0 || own > 4 || own == 3 || g_bid2_own >= 0) return;
        if (std::system("/system/vendor/cinder/bin/cinder-power usbid3") != 0) {
            clog_("suspend: could not set the USB board ID — USB will need a restart after this suspend");
            return;
        }
        g_bid2_own = own;
    } else if (g_bid2_own >= 0) {
        std::snprintf(cmd, sizeof cmd, "/system/vendor/cinder/bin/cinder-power usbid%ld", g_bid2_own);
        if (std::system(cmd) == 0) g_bid2_own = -1;   // else: try again next tick
    }
}

} // namespace socsusp

// Leave stage 1 NOW, from the wake path. The tick would get there within a second, but the panel
// cannot light until late resume has re-initialised the LCM (~0.3 s), so every tick of delay is
// time the user spends pressing Power at a black screen. Same order as the tick's exit: lock first.
// A KEY WHILE THE KERNEL IS GOING TO SLEEP. The lock is released and the kernel takes a second
// or more to suspend, and Cinder still reads keys in that window. 11-19: Play 1.3 s after the
// release started playback, the kernel then suspended under it, the audio looped and the player
// hung until a forced restart. Taking the lock back before the key acts aborts the suspend
// (activating a wakeup source mid-suspend is a pending wakeup to the kernel). Stays in stage 1.
static void soc_suspend_hold_for_input() {
    using namespace socsusp;
    if (!g_ram_released) return;
    g_ram_released = false;
    write_node(kWakeLock, kLockName);
    clog_("suspend: a key during suspend entry -> wakelock taken back first");
}

static void soc_suspend_leave_now(const char* why) {
    using namespace socsusp;
    if (!g_early) return;
    write_node(kWakeLock, kLockName);
    write_node(kStateNode, "on");
    g_early = false;
    g_ram_released = false;
    char m[96];
    std::snprintf(m, sizeof m, "suspend: %s -> left early suspend", why);
    clog_(m);
}

// ── THE USB PORT AFTER A SUSPEND TO RAM ─────────────────────────────────────────────────────────
// The controller resumes as an idle host and a cable plugged in afterwards reaches nobody (the
// 2026-09-04 fault, seen again 2026-10-04). Once a cable is in after a resume, give the port five
// seconds to come up by itself and then run `cinder-msc usb-resume`, which tries the driver's
// cable switch, then the gadget, then init — and logs each step, because with USB down the log is
// the only witness. Once per resume. system() on this thread for a few seconds, with the screen
// dark and nothing playing; the port is useless until it runs. A wall charger is a cable with no
// host behind it: see usb_resume_gate.h (#52) for the budget and the one-try-per-plug rule.
static bool g_usb_after_resume = false;
static int g_usb_resume_rc = 0;
static void usb_after_resume_tick() {
    static int plugged_s = 0;
    static bool forced = false;   // usb-resume wrote `1` (device) to mt_usb/mode
    static bool gave_up = false;  // a run on this plug found no host (a charger): not again till unplugged
    const bool cable = socsusp::read_node_long("/sys/class/power_supply/usb/online") == 1;
    if (!cable) gave_up = false;
    if (forced && !cable) {
        // Sony's SetOtgIdle on the way out, so the forced role does not outlive the cable.
        forced = false;
        socsusp::write_node("/sys/devices/platform/mt_usb/mode", "0");
        clog_("usb: cable out after a forced device role -> role released");
    }
    if (!cinder_usb_resume_due(&plugged_s, g_usb_after_resume, cable, gave_up)) return;
    g_usb_after_resume = false;
    forced = true;
    clog_("usb: a cable is in after a resume from RAM -> cinder-msc usb-resume");
    g_usb_resume_rc = -1;
    run_watchdog_only("usb: resume", USB_RESUME_BUDGET_S, []() {   // system()
        g_usb_resume_rc = std::system("/system/vendor/cinder/bin/cinder-msc usb-resume");
    });
    if (g_usb_resume_rc != 0) {
        gave_up = true;
        clog_("usb: no host found (a charger, or a port only a restart brings back) -> not again "
              "until the cable comes out");
    }
}

// Called once a second from the housekeeping block. Returns nothing; all state is internal.
static void soc_suspend_tick(bool idle, bool ram_idle) {
    using namespace socsusp;
    // Boot grace measured from the first call, i.e. from when this app started running. That is
    // the number that matters: the guarantee is a window in which the device is reachable no
    // matter what this code does, and a long-running device that has just had cinder-home
    // restarted needs that window as much as a cold boot does.
    static long long first_tick_ms = 0;
    const long long nowms = now_ms();
    if (!first_tick_ms) first_tick_ms = nowms;
    const long uptime_s = (long)((nowms - first_tick_ms) / 1000);

    const int thr = threshold_s();
    if (!thr) return;                       // disabled — the default
    if (adv_on("no_suspend")) return;  // the escape hatch, checked every tick on purpose

    static long last_rc   = -1;
    static int  idle_secs = 0;
    bool& early           = g_early;  // stage 1 active: `mem` written, wakelock held
    if (!early) resume_as_device(false);   // any way out of stage 1 or 2 puts the board ID back

    // Detect a resume FIRST, before deciding anything else: the tick that observes a resume is the
    // one running in the awake window, and it may be the only one we get.
    const long rc = read_node_long(kResumeCount);
    if (last_rc < 0) last_rc = rc;
    if (rc >= 0 && rc != last_rc) {
        const unsigned r12 = (unsigned)read_node_long(kSpmR12);
        last_rc = rc;
        idle_secs = 0;
        // Came back from a full suspend. Take the wakelock straight away so the kernel cannot
        // drop us again before this loop has decided anything, then leave the state cleanly. The
        // panel is deliberately NOT touched: if a person woke this device they pressed something,
        // and the ordinary input path is already the thing that lights the screen.
        write_node(kWakeLock, kLockName);
        write_node(kStateNode, "on");
        early = false;
        g_ram_released = false;
        char m[88];
        std::snprintf(m, sizeof m, "suspend: resumed from RAM (r12=%#x) -> awake, countdown restarts", r12);
        clog_(m);
        g_usb_after_resume = true;   // the port comes back as an idle host: see usb_after_resume_tick
        // Woken by a side key? See g_keywake_pending. The driver has already tried and failed by
        // the time user space thaws; a short grace covers its event still being in the buffer.
        unsigned eint0 = 0;
        if (FILE* f = std::fopen("/sys/devices/platform/icx_pm_helper/eint_sta", "r")) {
            if (std::fscanf(f, "%x", &eint0) != 1) eint0 = 0;
            std::fclose(f);
        }
        if ((g_keywake_pending = (eint0 >> kKeyWakeEint) & 1)) {
            g_keywake_due_ms = now_ms() + 150;   // the driver gave up ~0.3 s before tasks thaw
            clog_("suspend: woken by a side key");
        }
        return;
    }

    // A CABLE PULLED DURING STAGE 1 IS NEVER SEEN BY THE USB PORT. Handler 9 of the chain,
    // `bq24262_wmport_early_suspend`, parks the WM-PORT VBUS thread, so the unplug is never
    // processed: no "Disconnect USB", no "PHY off", and the USB0 clock stays up — which alone blocks
    // deep idle (`dpidle_block_mask[CG_PERI0]=0x400`). Measured on Walkman One 2026-09-28: 150 s
    // off-cable in stage 1, `by_vtg` frozen as designed, `dpidle_cnt` 0 the whole time. The charger
    // node still sees it, so leave the chain long enough for late resume to run the port's handler
    // and come back. Late resume leaves the backlight at 0 (checked the same day), so the panel
    // stays dark. The ordinary order — unplug, pocket, stage 1 a minute later — never hits this.
    static long last_usb = -1;
    const long usb = read_node_long("/sys/class/power_supply/usb/online");
    if (early && last_usb == 1 && usb == 0) {
        write_node(kWakeLock, kLockName);
        write_node(kStateNode, "on");
        early = false;
        idle_secs = thr - 15;   // back in ~15 s, once the port has let go (negative under a short threshold)
        clog_("suspend: cable pulled during early suspend -> out for ~15 s so the USB port can "
              "power down (it holds deep idle off until it does)");
    }
    if (usb >= 0) last_usb = usb;

    // Busy, or too soon after start: undo stage 1 if we are in it and reset the clock.
    if (!idle || uptime_s < kBootGraceS) {
        if (early) {
            write_node(kWakeLock, kLockName);   // ensure we hold it before leaving the state
            write_node(kStateNode, "on");
            early = false;
            clog_("suspend: activity -> left early suspend");
        }
        idle_secs = 0;
        return;
    }

    if (idle_secs < 100000) ++idle_secs;
    if (idle_secs < thr) return;

    if (!early) {
        // A node that refuses the write refuses it every time. Back off 1 min -> x3 -> 1 h, and
        // log at the attempt, so a unit where this can never work costs a handful of lines an hour
        // rather than one a second (the harness's autooff-idle scenario caught exactly that once
        // this became the default).
        static long long fail_next_ms = 0;
        static long fail_gap_ms  = 0;
        if (fail_next_ms && nowms < fail_next_ms) return;
        // Stage 1. Wakelock FIRST — the order matters, because writing `mem` with no lock held
        // arms autosleep and the kernel can take the device to RAM before the next tick.
        write_node(kWakeLock, kLockName);
        if (write_node(kStateNode, "mem")) {
            early = true;
            fail_next_ms = 0;
            fail_gap_ms  = 0;
            char m[88];
            std::snprintf(m, sizeof m, "suspend: idle %d s -> early suspend (deep idle on, still awake)", idle_secs);
            clog_(m);
        } else {
            write_node(kWakeUnlock, kLockName);
            fail_gap_ms  = fail_gap_ms ? std::min(fail_gap_ms * 3, 3600000L) : 60000L;
            fail_next_ms = nowms + fail_gap_ms;
            char m[112];
            std::snprintf(m, sizeof m, "suspend: /sys/power/state write failed, staying awake "
                                       "(next try in %ld s)", fail_gap_ms / 1000);
            clog_(m);
        }
        return;
    }

    // ── STAGE 2, OPT-IN: SUSPEND TO RAM, AS SONY'S OWN FIRMWARE DOES WHEN IT IS IDLE ─────────────
    // analysis/RE_sony_idle_baseline.md: stock's PowerService writes `mem` with no lock held once
    // every audio source has agreed, and holds a lock whenever USB or the charger is in. Stage 1 is
    // deep idle with user space still running (WMPortService, khubd_poll and friends wake the CPU
    // about 35 times a second, measured off the cable 2026-10-04); suspended, nothing runs at all.
    //
    // `ram_idle` is Sony's interlock set, decided by the caller: nothing audible on ANY route (stage
    // 1 may run under music, this must not — on Bluetooth no PCM is open, so no kernel audio lock
    // would stop it), no Bluetooth peer and no connect pending, not receiving, not exporting the
    // drive, and off both the cable and the charger. The cable test is belt and braces: the charger
    // driver holds its own wakelock while VBUS is present, so the kernel would refuse anyway.
    //
    // kRamDwellS of stage 1 first: 10 s, the delay Sony's own app uses before its cold sleep
    // (analysis/RE_power_stock_vs_cinder.md section 3). The file is read each time, so deleting
    // it stops it.
    static int ram_dwell = 0;
    // On by default on both channels since 2026-10-24 (owner's decision), `no_ram_suspend` turns it
    // off. USB after a resume (checklist 33.2) is still partial; resume_as_device and
    // `cinder-msc usb-resume` are the fix in progress.
    const bool ram_on = !adv_on("no_ram_suspend");
    if (!ram_idle || !ram_on) { ram_dwell = 0; return; }
    if (++ram_dwell < kRamDwellS) return;
    ram_dwell = 0;
    clog_("suspend: idle and off the cable -> releasing the wakelock (suspend to RAM; Power wakes it)");
    resume_as_device(true);
    g_ram_released = true;
    g_ram_rc = read_node_long(kResumeCount);
    g_ram_at_ms = now_ms();
    write_node(kWakeUnlock, kLockName);
}
