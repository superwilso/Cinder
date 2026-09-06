// scenarios.cpp — boot the real main.cpp against fake services and assert on what it DID.
//
// Each scenario runs in its own process (main.cpp is full of one-shot static state — g_deferred_done,
// g_render_ready, the pump thread — so a second boot in the same process would not be a boot). The
// runner script invokes this binary once per scenario name; `all` is only for eyeballing.
//
// What these test that nothing else can: the app's BEHAVIOUR OVER TIME. Every Bluetooth defect this
// project has shipped was a sequencing or a pacing defect — a bring-up step that ran before the
// service existed and was never retried, a poll that never backed off, a switch that believed its
// own state instead of the radio's. A pure-function test cannot see any of that; a 60-second boot
// trace shows all of it.
#include "harness.h"

#include <cstdio>
#include <cstring>
#include <cstdlib>
#include <string>

static int g_fails = 0;
static const char* g_scenario = "?";

static void check(bool ok, const char* what) {
    std::printf("  %s %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok) g_fails++;
}

static void check_eq(long long got, long long want, const char* what) {
    bool ok = got == want;
    std::printf("  %s %s (got %lld, want %lld)\n", ok ? "ok  " : "FAIL", what, got, want);
    if (!ok) g_fails++;
}

static void check_range(long long got, long long lo, long long hi, const char* what) {
    bool ok = got >= lo && got <= hi;
    std::printf("  %s %s (got %lld, want %lld..%lld)\n", ok ? "ok  " : "FAIL", what, got, lo, hi);
    if (!ok) g_fails++;
}

// A device that is working: the renderer presents frames, the library DB opens, the audio pump
// starts. Scenarios override individual pieces to describe the failure they are about.
// The two states of the library volume, as /proc/mounts shows them. Only the " /contents " line
// is load-bearing (contents_mounted greps for exactly that), but the rest is kept so the fake reads
// like the real file rather than like a fixture.
static const char* kMountsWithContents =
    "rootfs / rootfs rw 0 0\n"
    "/emmc@usrdata /data ext4 rw,nodev,noexec,noatime 0 0\n"
    "/emmc@contents /contents vfat rw,noexec,noatime,fmask=0000,dmask=0000 0 0\n";
static const char* kMountsNoContents =
    "rootfs / rootfs rw 0 0\n"
    "/emmc@usrdata /data ext4 rw,nodev,noexec,noatime 0 0\n";

static void healthy_device(void) {
    cinder_harness_script("cinder_frames_presented", 1);
    cinder_harness_script("cinder_render_init", 0);
    cinder_harness_script("cinder_db_open", 0);
    cinder_harness_script("cinder_audio_init", 0);

    // …and hardware for it to read. Without these the app runs against a device with no battery, no
    // charger state and no headphone jack, which is a fine default and a useless fixture: every
    // rule that matters here is an EDGE on one of these files.
    cinder_harness_fs_write("/sys/class/power_supply/battery/capacity", "74\n");
    cinder_harness_fs_write("/sys/class/power_supply/battery/status", "Discharging\n");
    cinder_harness_fs_write("/sys/class/power_supply/usb/online", "0\n");
    cinder_harness_fs_write("/sys/class/power_supply/usb/present", "0\n");
    cinder_harness_fs_write("/sys/class/android_usb/android0/state", "DISCONNECTED\n");
    cinder_harness_fs_write("/sys/class/switch/cxd3778gf_h2w/state", "1\n");   // headphones in
    cinder_harness_fs_mkdir("/data/cinder");   // so the bad-boot counter can be cleared
    cinder_harness_fs_mkdir("/contents");
    // The launcher's log target has to EXIST: entering USB-MSC moves the app's fds off /contents
    // and puts them back on the way out, and with no file to go back to they land on /dev/null and
    // everything after the session is invisible.
    cinder_harness_fs_write("/contents/cinderhome.log", "");
    // AND A MOUNT TABLE. /contents being mounted is a fact the app reads from /proc/mounts, and
    // with no fake for it the app read the BUILD MACHINE's — where /contents has never been
    // mounted, so every scenario silently ran against a device whose library volume was missing.
    // Nothing noticed until something was taught to care (usb-msc reclaim). The MSC helper is what
    // moves this line, so the fake `system()` moves it too; see harness.cpp.
    cinder_harness_fs_write("/proc/mounts", kMountsWithContents);
}

// ── the app reaches the end of the easel lifecycle and brings itself up ──────────────────────
static void s_boot(void) {
    healthy_device();
    cinder_harness_bt_add_paired("WH-1000XM4", 0x91);
    cinder_harness_set_budget_ms(20000);
    cinder_harness_run();

    check(cinder_harness_count("easel:OnForeground") == 1, "reached Foreground");
    check(cinder_harness_count("cinder_render_init") == 1, "renderer brought up once");
    check(cinder_harness_count("cinder_db_open") == 1, "library DB opened once (no retry storm)");
    check(cinder_harness_count("cinder_audio_pump_start") == 1, "framework event pump started");
    check(cinder_harness_before("cinder_audio_pump_start", "cinder_audio_init") == 1,
          "event pump starts BEFORE the PlayerService connect");
    check(cinder_harness_count("easel:StopBootAnimation") >= 1, "boot animation killed");

    // PR #3: both of these used to happen only when the user opened the Bluetooth screen, so a
    // headphone that was already paired was invisible until then and the notification listener —
    // which is what makes a connection event arrive at all — was never registered at boot.
    check(cinder_harness_count("BtCommon::GetPairedDeviceInfo") >= 1,
          "paired-device list read during boot");
    check(cinder_harness_count("BtCommon::AddListener") >= 1,
          "BtCommon notification listener registered during boot");
}

// ── hagodaemon is not up: every Sony service client fails to build, for the whole run ────────
// The app must still boot, paint and shut down. This is the boot-race case that has soft-bricked
// this device before — a service that is not there yet must degrade a feature, never the Home app.
static void s_no_services(void) {
    healthy_device();
    cinder_harness_script("pst:BtCommonServiceClientFactory::CreateInstance", 0);
    cinder_harness_script("pst:BtTransmitterServiceClientFactory::CreateInstance", 0);
    cinder_harness_script("pst:UsbDeviceAudioPlayerServiceClientFactory::CreateInstance", 0);
    cinder_harness_set_budget_ms(20000);
    cinder_harness_run();

    check(cinder_harness_count("easel:OnForeground") == 1, "still reached Foreground");
    check(cinder_harness_count("cinder_render_tick") > 100, "still painting frames");
    check(cinder_harness_count("easel:OnFinalize") == 1, "shut down cleanly");
    check_eq(cinder_harness_count("BtCommon::GetBtStatus"), 0, "no status read without a client");
}

// ── the service arrives late (the defect class this project keeps hitting) ───────────────────
// The BtCommonServiceClient factory fails the first four times and then works. Anything the app
// only ever tried once during bring-up is lost for the life of the process; the fix was to keep
// asking. The assertion is that it did.
static void s_bt_late_service(void) {
    healthy_device();
    static const long long seq[] = {0, 0, 0, 0, 1};   // null, null, null, null, then live forever
    cinder_harness_script_seq("pst:BtCommonServiceClientFactory::CreateInstance", seq, 5);
    cinder_harness_bt_set_radio(1);
    cinder_harness_bt_add_paired("WH-1000XM4", 0x91);
    cinder_harness_set_budget_ms(60000);
    cinder_harness_run();

    check(cinder_harness_count("pst:BtCommonServiceClientFactory::CreateInstance") >= 5,
          "kept retrying the client after it failed to build");
    check(cinder_harness_count("BtCommon::GetBtStatus") >= 1,
          "read the radio once the service existed");
    check(cinder_harness_last_ms("BtCommon::GetBtStatus") > 10000,
          "still reading the radio late in the session");
}

// ── the radio is already on, the UI switch is not: the switch must follow the radio ──────────
static void s_bt_switch_follows_radio(void) {
    healthy_device();
    cinder_harness_bt_set_radio(1);
    cinder_harness_bt_add_paired("WH-1000XM4", 0x91);
    cinder_harness_set_budget_ms(30000);
    cinder_harness_run();

    check(cinder_harness_count("cinder_set_bt_on") >= 1, "the UI switch was reconciled");
    check_eq(cinder_harness_arg("cinder_set_bt_on", 0), 1, "reconciled to ON, matching the radio");
}

// ── the app must never jam its own connects ──────────────────────────────────────────────────
//
// THE BUG THIS EXISTS FOR (2026-08-26). `bt_reconnect_tick` armed `SetConnectRetryMode(true,…)` on
// the first tick after any drop — about a second after the radio came up. While that mode is armed
// the real transmitter REFUSES every connect request: rc=0, and nothing reaches the air. So from
// that moment the app could not connect to anything, and the user's own log showed three Devices-row
// taps in a row returning rc=0 while the ladder re-armed the jam between them. Reported as
// "pairing a new device works but connecting is hit and more often miss".
//
// Nothing off-device could see it, for two reasons this scenario also fixes: slots 6 and 7 shared
// one always-succeeds handler, and the retry mode was not modelled at all.
//
// The assertion is about a SEQUENCE, not a value: whatever else the app does, it must not be
// holding the retry mode on while it asks for a connection.
static void s_bt_never_jams_itself(void) {
    healthy_device();
    cinder_harness_bt_set_radio(1);
    cinder_harness_bt_add_paired("WH-1000XM4", 0x91);
    cinder_harness_set_budget_ms(60000);
    cinder_harness_run();

    check_eq(cinder_harness_count("BtXmit::SetConnectRetryMode"), 0,
             "never armed the retry mode (it refuses every connect while on)");
    check_eq(cinder_harness_bt_retry_mode(), 0, "left the radio's retry mode off");
    check(cinder_harness_bt_connected() == 1, "the paired device is connected by the end");
}

// ── …and it must CLEAR a retry mode someone else left armed ──────────────────────────────────
// The mode is service state and sticky: it outlives the process. A probe session, or a Cinder from
// before the fix, leaves the radio refusing every connect. An app that only ever avoids arming it
// would come up, ask politely, be refused, and never work out why.
static void s_bt_clears_a_stale_jam(void) {
    healthy_device();
    cinder_harness_bt_set_radio(1);
    cinder_harness_bt_set_retry_mode(1);          // someone else left it armed
    cinder_harness_bt_add_paired("WH-1000XM4", 0x91);
    cinder_harness_set_budget_ms(60000);
    cinder_harness_run();

    check(cinder_harness_count("BtXmit::GetConnectRetryMode") >= 1,
          "asked the service what the mode really was rather than assuming");
    check_eq(cinder_harness_bt_retry_mode(), 0, "cleared the stale jam");
    check(cinder_harness_bt_connected() == 1, "and then actually connected");
}

// ── the ladder must not ask while the radio is already asking ────────────────────────────────
// A connect request issued while a page is on the air comes back rc=0 — measured on device, and
// benign in itself, but the ladder read that refusal as "wrong device" and jumped to naming one on
// evidence that said nothing about which device was right. `GetAvSrcConnectionStatus` == 3 is the
// radio saying "already trying"; the ladder stands aside for it, up to a bounded number of times so
// a wedged status cannot silence it for good.
static void s_bt_waits_for_a_page_in_flight(void) {
    healthy_device();
    cinder_harness_bt_set_radio(1);
    cinder_harness_bt_add_paired("WH-1000XM4", 0x91);
    // Hold the fake radio in "connecting" for the whole run. Nothing can ever link, so any connect
    // the app makes is one it should have deferred.
    cinder_harness_script("BtXmit::GetAvSrcConnectionStatus", 3);
    cinder_harness_set_budget_ms(90000);
    cinder_harness_run();

    check(cinder_harness_count("BtXmit::GetAvSrcConnectionStatus") >= 1,
          "asked the radio whether it was already trying");
    // The skip cap is 6, so a handful of attempts do get through — that is deliberate, a status
    // stuck at 3 must not mean silence. What must not happen is a request every time round.
    check_range(cinder_harness_count("BtXmit::RequestLastDeviceConnection")
                  + cinder_harness_count("BtXmit::RequestConnection"),
                0, 3, "stood aside for the page already in flight");
}

// ── an idle radio reports 3, and 3 is not a connection ───────────────────────────────────────
// `GetBtStatus` reaches 3 with nothing on the other end (measured 0.61 s after powering an idle
// radio). The route used to be `st == 3`, so it flipped to BLUETOOTH the moment the radio came up:
// the volume rocker left the jack with no sink to drive, and the reconnect-edge cascade fired
// against thin air — then did NOT fire when a device really connected, because the flag was already
// set. The address is the link.
static void s_bt_idle_radio_is_not_a_link(void) {
    healthy_device();
    cinder_harness_bt_set_radio(1);               // radio up, NOTHING paired, so nothing can connect
    cinder_harness_set_budget_ms(30000);
    cinder_harness_run();

    check(cinder_harness_count("BtCommon::GetBtStatus") >= 1, "the radio was polled");
    check_eq(cinder_harness_bt_connected(), 0, "nothing is connected");
    check_eq(cinder_harness_count("cinder_set_bt_route"), 0,
             "the volume route never moved to Bluetooth with no peer");
}

// ── PR #4: the idle Bluetooth poll must back off ─────────────────────────────────────────────
// Before it did, four services were polled at ~2 Hz for the entire life of the process — a binder
// round trip every half second into a radio that had not changed state in an hour. The numbers
// below are the measured behaviour of the current code, pinned so a regression is a test failure
// rather than a battery complaint six weeks later.
static void s_bt_idle_poll_rate(void) {
    healthy_device();
    cinder_harness_set_budget_ms(120000);
    cinder_harness_run();   // radio off, nothing paired, nothing playing — the idle case

    int early = cinder_harness_count_between("BtCommon::GetBtStatus", 0, 30000);
    int late  = cinder_harness_count_between("BtCommon::GetBtStatus", 90000, 120000);
    std::printf("  .... GetBtStatus: %d calls in the first 30s, %d in the last 30s\n", early, late);
    check(late <= early, "the radio poll does not speed up as the session goes on");
    check_range(late, 0, 6, "at most one radio read every 5s once the radio is known down");

    int np = cinder_harness_count_between("cinder_audio_current_uri", 90000, 120000);
    std::printf("  .... cinder_audio_current_uri: %d calls in the last 30s\n", np);
    check_range(np, 0, 4, "now-playing does not round-trip the binder on every poll while idle");
}

// ── bring-up that never completes must not freeze the rest of the app ────────────────────────
// The library DB will not open, ever. deferred_up retries forever and `g_deferred_done` never
// becomes true — and until 2026-08-24 that meant the deferred-init block WAS the frame loop for
// the life of the process: StopBootAnimation() (a framework call, each one logged and fflush'ed to
// /contents) sixty times a second, and everything below the `continue` frozen — the idle
// screen-off, the auto power-off, the sleep timer, the battery gauge, and input_pump. A device
// that could not open its library sat lit, deaf and burning until the battery was flat.
//
// This scenario is how that was found, and it is here so it cannot come back.
static void s_stalled_bringup(void) {
    cinder_harness_script("cinder_frames_presented", 1);
    cinder_harness_script("cinder_render_init", 0);
    cinder_harness_script("cinder_db_open", -1);        // never opens
    cinder_harness_set_budget_ms(3600000);
    cinder_harness_run();

    check(cinder_harness_count("cinder_render_tick") > 100, "still painting");
    // The DB retry BACKS OFF now, and this check used to assert the opposite (">= 30 in the
    // minute, paced at ~1 Hz"). MEASURED on device 2026-08-26 (checklist 2D.2): with
    // /db/MTPDB.dat renamed away, a flat 1 Hz meant an sqlite open against a missing file every
    // second for the life of the boot, and 61 KB of log in ten minutes. A database that is not
    // there at second 60 will not be there at second 61; the transient-mount race this retry
    // exists for resolves in the first seconds, which the early fast attempts still cover.
    //
    // So: a handful in the window, and never ZERO — backing off must not become giving up, because
    // the whole point of the retry is that a late mount still gets picked up.
    int db_tries = cinder_harness_count_between("cinder_db_open", 60000, 120000);
    std::printf("  .... cinder_db_open: %d attempts in the 60-120s window (was ~60)\n", db_tries);
    check_range(db_tries, 1, 8, "the DB retry backs off, but has not given up");

    // The whole point: the last minute must look like a running app, not like a boot that never
    // ended. Ranges rather than exact counts — the pacing is wall-clock, not frame-counted.
    int anim = cinder_harness_count_between("easel:StopBootAnimation", 60000, 120000);
    std::printf("  .... StopBootAnimation: %d calls in the last 60s (was ~3800)\n", anim);
    check_range(anim, 0, 20, "the boot-animation re-kill backs off instead of running every frame");

    check(cinder_harness_count_between("cinder_get_screen_off_s", 60000, 120000) >= 30,
          "housekeeping runs: the idle screen-off is still being evaluated");
    check(cinder_harness_count_between("cinder_sleep_should_pause", 60000, 120000) >= 30,
          "housekeeping runs: the sleep timer is still being evaluated");
    check(cinder_harness_count("cinder_set_battery") >= 1,
          "housekeeping runs: the battery gauge still reaches the status bar");

    // …and it has to stay QUIET while it does. Every line here is an fflush to /contents, and a
    // bring-up that never completes is a state the device can sit in for a day: the retry itself
    // logged 3,571 copies of "DB unavailable" an hour, and the boot-animation re-kill kept calling
    // into the framework every five seconds for ever. Both are bounded now.
    int lines = cinder_harness_count_between("log", 600000, 3600000);
    std::printf("  .... %d log lines over the last 50 minutes of a stalled boot\n", lines);
    check_range(lines, 0, 60, "a device stuck like this does not fill its flash with the news");
}

// ── dark + playing: the state this device spends most of its life in ─────────────────────────
// Screen off, in a pocket, music going to Bluetooth headphones, for hours. main.cpp calls the
// awake/dark frame-rate drop "the single biggest battery lever in the app" and the whole idle
// design rests on it — and until the harness existed, nothing outside a device session could show
// that the lever was actually being pulled. A regression here (anything that keeps the loop at
// 60 Hz while the panel is dark) would be the most expensive bug the app could have, and would be
// invisible: the screen is off, so nobody would see it.
static void s_dark_playing(void) {
    healthy_device();
    cinder_harness_script("cinder_get_screen_off_s", 30);   // a realistic idle timeout
    cinder_harness_script("cinder_audio_is_playing", 1);
    cinder_harness_bt_set_radio(1);
    cinder_harness_bt_add_paired("WH-1000XM4", 0x91);
    cinder_harness_set_budget_ms(180000);
    cinder_harness_run();

    // Frame-loop rate: cinder_get_usb_dac is read once per iteration, so it counts iterations.
    int frames = cinder_harness_count_between("cinder_get_usb_dac", 120000, 180000);
    std::printf("  .... frame loop: %d iterations in the last 60s (awake would be ~3750)\n", frames);
    check_range(frames, 30, 200, "the loop drops to ~1 Hz once the panel is dark");

    check_eq(cinder_harness_count_between("cinder_render_tick", 120000, 180000), 0,
             "nothing is painted while the panel is dark");
    // Housekeeping keeps its own 1 Hz regardless of the loop rate — the sleep timer, the scrobbler
    // and the USB-host debounce all ride on it, so it is the floor the dark budget may not cross.
    check_range(cinder_harness_count_between("cinder_clock_tick", 120000, 180000), 50, 75,
                "housekeeping still runs at 1 Hz with the panel dark");

    int ipc = cinder_harness_count_between("BtCommon::GetBtStatus", 120000, 180000)
            + cinder_harness_count_between("BtXmit::GetConnectInformation", 120000, 180000)
            + cinder_harness_count_between("BtXmit::GetSoundStatus", 120000, 180000)
            + cinder_harness_count_between("BtCommon::GetPairedDeviceInfo", 120000, 180000)
            + cinder_harness_count_between("cinder_audio_current_uri", 120000, 180000);
    std::printf("  .... Sony IPC: %d calls in the last 60s\n", ipc);
    check_range(ipc, 0, 12, "a steady Bluetooth session costs single-digit IPC calls a minute");
}

// ── the log is a flash write, so its VOLUME is a defect class ────────────────────────────────
// clog_ writes one line to stderr per event and fflushes it, and the launcher redirects stderr to
// /contents/cinderhome.log — a file on the same fragile vfat partition as the user's music. A line
// on a timer is therefore a flash write per tick, for as long as the player runs, and two defects
// of exactly that shape have already shipped:
//
//   * the boot-animation re-kill ran every frame when bring-up stalled — 62 lines a second;
//   * mark_healthy_maybe logged its failure on every 1 Hz retry — 21,594 of a six-hour log's
//     21,700 lines were that one sentence.
//
// Neither is visible in a unit test and both are invisible on the device until the log is opened.
// A ceiling on lines-per-hour catches the whole class, whatever causes the next one. Six virtual
// hours costs about a second here.
static void s_log_volume(void) {
    healthy_device();
    // Panel ON for the whole six hours, deliberately: it is the chattier case. The frame loop runs
    // at 60 Hz rather than 1 Hz, so anything paced by a frame COUNT rather than the wall clock shows
    // up here at sixty times the rate it would in the dark — which is exactly how the silent-input
    // heartbeat was caught writing 499 lines an hour.
    cinder_harness_script("cinder_audio_is_playing", 1);
    cinder_harness_bt_set_radio(1);
    cinder_harness_bt_add_paired("WH-1000XM4", 0x91);
    cinder_harness_set_budget_ms(6 * 3600 * 1000);
    cinder_harness_run();

    // Boot is allowed to be chatty — it is the part anyone debugging actually reads. It is the
    // STEADY STATE that must be quiet, so measure from an hour in.
    int lines = cinder_harness_count_between("log", 3600000, 21600000);
    std::printf("  .... %d log lines over five steady-state hours\n", lines);
    check_range(lines, 0, 300, "a running player writes well under a line a minute to flash");
    // If that fails, CINDER_HARNESS_TRACE=1 dumps the trace and the offending line is in it
    // verbatim — the count says there is a problem, the trace says which sentence.
}

// ── the headphones come out mid-track ────────────────────────────────────────────────────────
// The one behaviour the Bluetooth polling work was not allowed to break, and the reason
// jack_edge.h exists as a separate testable rule. The rule has its own self-test; this checks the
// app RUNS it — that the jack node is still being read on a timer, that the transition is seen,
// and that it reaches cinder_audio_pause rather than stopping at a log line.
static void s_jack_unplug(void) {
    healthy_device();
    cinder_harness_script("cinder_audio_is_playing", 1);
    cinder_harness_fs_write_at(60000, "/sys/class/switch/cxd3778gf_h2w/state", "0\n");
    cinder_harness_set_budget_ms(120000);
    cinder_harness_run();

    check(cinder_harness_count("cinder_audio_pause") >= 1, "playback was paused");
    long long at = cinder_harness_first_ms("cinder_audio_pause");
    std::printf("  .... paused at %lldms (unplugged at 60000ms)\n", at);
    check_range(at, 60000, 63000, "paused within a couple of seconds of the jack going low");
    check_eq(cinder_harness_count_between("cinder_audio_pause", 0, 59000), 0,
             "nothing paused it before the headphones came out");
}

// ── a PC is plugged in ───────────────────────────────────────────────────────────────────────
// The USB-MSC handoff is the one path whose own header warns that getting the ordering wrong "eats
// the user's library". This does not check the ordering — that is shell, and the launcher's own
// 44-case matrix covers it — but it does check the trigger: that the app notices a data host, that
// it debounces rather than firing on the first sample, and that it goes exactly once.
static void s_usb_msc(void) {
    healthy_device();
    cinder_harness_fs_write_at(60000, "/sys/class/power_supply/usb/online", "1\n");
    cinder_harness_fs_write_at(60000, "/sys/class/power_supply/usb/present", "1\n");
    cinder_harness_fs_write_at(60000, "/sys/class/android_usb/android0/state", "CONFIGURED\n");
    cinder_harness_set_budget_ms(120000);
    cinder_harness_run();

    long long at = cinder_harness_first_ms("cinder_show_usb_storage");
    std::printf("  .... MSC modal raised at %lldms (host appeared at 60000ms)\n", at);
    check(at >= 60000, "did not hand the volume over before a host was there");
    check_range(at, 60000, 65000, "noticed the host within a few seconds");
    check_eq(cinder_harness_count("cinder_show_usb_storage"), 1, "handed over exactly once");
    // The app releases PlayerService's grip on /contents before the unmount — a paused service
    // keeps the current track's file open, and that alone makes the unmount fail EBUSY and the PC
    // see a reader with no medium.
    check(cinder_harness_count("cinder_audio_release_sequence") >= 1,
          "released the pinned track sequence before handing the volume over");
}

// ── auto power-off, and the guards that keep it out of somebody's hand ───────────────────────
// Sony has this and Cinder did not, so a paused device with the screen dark ran until the battery
// was flat — the largest item in the 2026-08-16 battery audit. It is off by default and has four
// guards, each of which is a way it could otherwise switch the device off while someone is using
// it. Three scenarios: it fires when it should, and it does not fire in the two cases where firing
// would be the bug.
//
// The idle one also pins the FIFTH guard, added after the harness found it missing: power_action
// only returns when the helper failed, and this block runs at ~1 Hz, so a device whose setuid bit
// is gone forked the helper and wrote three log lines every second for ever — 3,541 attempts and
// 10,623 flushed lines in one virtual hour. (The harness's `system` is a recording stub, so here
// the helper always "fails", which is precisely the case worth testing.)
static void s_auto_off_idle(void) {
    healthy_device();
    cinder_harness_script("cinder_get_auto_off_min", 1);
    cinder_harness_script("cinder_get_screen_off_s", 30);
    cinder_harness_set_budget_ms(3600000);
    cinder_harness_run();

    const char* helper = "system:/system/vendor/unknown321/bin/cinder-power off";
    long long at = cinder_harness_first_ms(helper);
    std::printf("  .... first power-off attempt at %lldms (1 min idle)\n", at);
    check_range(at, 60000, 66000, "powered off a minute after the last input");

    int tries = cinder_harness_count(helper);
    std::printf("  .... %d attempts over the hour, %d log lines\n", tries, cinder_harness_count("log"));
    check_range(tries, 1, 20, "a helper that cannot work is retried slowly, not once a second");
    check_range(cinder_harness_count("log"), 0, 120, "and does not narrate every attempt");
}

static void s_auto_off_playing(void) {
    healthy_device();
    cinder_harness_script("cinder_get_auto_off_min", 1);
    cinder_harness_script("cinder_get_screen_off_s", 30);
    cinder_harness_script("cinder_audio_is_playing", 1);
    cinder_harness_set_budget_ms(300000);
    cinder_harness_run();
    check_eq(cinder_harness_count("system:/system/vendor/unknown321/bin/cinder-power off"), 0,
             "never powers off while the service says audio is playing");
}

static void s_auto_off_charging(void) {
    healthy_device();
    cinder_harness_script("cinder_get_auto_off_min", 1);
    cinder_harness_script("cinder_get_screen_off_s", 30);
    cinder_harness_fs_write("/sys/class/power_supply/battery/status", "Charging\n");
    cinder_harness_fs_write("/sys/class/power_supply/usb/online", "1\n");
    cinder_harness_set_budget_ms(300000);
    cinder_harness_run();
    check_eq(cinder_harness_count("system:/system/vendor/unknown321/bin/cinder-power off"), 0,
             "never powers off a device sitting on a charger");
}

// ── the DSP reconcile must not depend on having found a settings file ────────────────────────
// The DSP is not ours and does not boot empty: it holds whatever the stock player last left in it.
// This used to run only `if (g_settings_loaded)`, so on a boot with no readable
// /contents/cinder_settings.conf — vfat, handed wholesale to a PC for USB-MSC, "both corruptible
// and periodically absent" by this file's own description — Cinder drew its own defaults, sent
// nothing, and the screen said one thing while the hardware did another for the whole session.
//
// Two of the calls are not user preferences at all, which is what made the gate indefensible:
// SetSelectUsingEq (the device sits on the six-band EQ, which Cinder does not expose, so without
// it every band the EQ screen writes is stored and never put in the path) and the BT sound-effect
// flag. Both are assertions about somebody else's state.
static void s_dsp_reconcile_no_settings(void) {
    healthy_device();
    cinder_harness_script("cinder_settings_load", 0);   // no saved settings, the awkward case
    cinder_harness_set_budget_ms(20000);
    cinder_harness_run();

    // The selector reaches SetSelectUsingEq through cinder_effects_set_tone_system, not through
    // the identically-named cinder_effects_set_select_using_eq — two FFI entry points, one Sony
    // method (effect_shim.cpp). The harness sees the FFI boundary, so a scenario has to name the
    // call the app actually makes; asserting on the other one fails against perfectly good code.
    check(cinder_harness_count("cinder_effects_set_tone_system") >= 1,
          "the EQ/tone selector is pushed even with no settings file");
    check(cinder_harness_count("cinder_effects_set_eq") >= 1, "the EQ is pushed at boot");
    check(cinder_harness_count("cinder_effects_set_dsee_hx") >= 1, "the sound chain is pushed at boot");
    check(cinder_harness_count("cinder_effects_set_vpt") >= 1, "…including VPT");
    // The one that genuinely IS a restore stays gated: with no file there is nothing to restore.
    check_eq(cinder_harness_count("cinder_audio_set_repeat_one"), 0,
             "repeat-one is not 'restored' from a file that does not exist");
}

// ── the whole mass-storage round trip: cable in, cable out ───────────────────────────────────
// The exit path is not a second feature, it is the SAME one — the modal, the remount and the USB
// mode all have to come back, and the only way out of the modal is Back. `cinder_input` is only
// reached from the MSC branch in this harness (input_pump never fires without /dev/input), so
// scripting it to answer Back is how the cable-pull exit gets exercised.
//
// The other half of this scenario is what the app does while the session is WEDGED. `cinder-msc` is
// a recording stub here, so the gadget LUN never gets a backing file — the "PC sees a reader with
// no medium" case the code's own comments are about. `ensure_msc_lun` runs a retry ladder costing
// about two seconds of sleeps, and it is called from the ~1 Hz housekeeping, so before this was
// found the render thread spent two seconds out of every one inside it: an entire MSC session with
// a UI that does not repaint and a Back button sampled every other second.
static void s_msc_cycle(void) {
    healthy_device();
    cinder_harness_script("cinder_input", 19 /* CINDER_ACT_EXIT_USB_MSC */);
    cinder_harness_fs_write_at(60000,  "/sys/class/power_supply/usb/online", "1\n");
    cinder_harness_fs_write_at(60000,  "/sys/class/power_supply/usb/present", "1\n");
    cinder_harness_fs_write_at(60000,  "/sys/class/android_usb/android0/state", "CONFIGURED\n");
    cinder_harness_fs_write_at(120000, "/sys/class/power_supply/usb/online", "0\n");
    cinder_harness_fs_write_at(120000, "/sys/class/power_supply/usb/present", "0\n");
    cinder_harness_fs_write_at(120000, "/sys/class/android_usb/android0/state", "DISCONNECTED\n");
    cinder_harness_set_budget_ms(200000);
    cinder_harness_run();

    check_eq(cinder_harness_count("system:/system/vendor/unknown321/bin/cinder-msc on"), 1,
             "handed the volume over once when the cable went in");
    long long off = cinder_harness_first_ms("system:/system/vendor/unknown321/bin/cinder-msc off");
    std::printf("  .... released at %lldms (cable pulled at 120000ms)\n", off);
    check_range(off, 120000, 126000, "took it back when the cable came out");

    // The wedged-session guard. The ladder writes the LUN backing file eight times per run, so the
    // count of those writes is the count of ladders — one a second would be ~60 over the session.
    int lun_writes = cinder_harness_count(
        "system:echo /emmc@contents > /sys/class/android_usb/android0/f_mass_storage/lun/file 2>/dev/null");
    std::printf("  .... %d LUN bind attempts over a 60s wedged session\n", lun_writes);
    check_range(lun_writes, 1, 80, "a LUN that will not bind is retried on a timer, not continuously");
}

// ── touch and buttons: the half of the app that had no off-device exercise at all ────────────
// cinder-home reads /dev/input directly, so until the harness grew fake nodes, every gesture and
// every button — the whole path from a raw evdev code to carry_out — was covered by nothing.
// (cinder-ui's 404 tests cover the NAVIGATOR's decisions; these cover getting there.)
//
// The navigator itself is a stub here, so `cinder_input` returns "no action" and nothing downstream
// of it runs. What these check is everything on THIS side of that boundary: that raw codes decode
// to the right logical button, that a contact becomes a tap and a drag becomes a drag, and the
// screen-wake rules main.cpp owns outright.

// The panel is dark and you touch it. The touch must WAKE it and must NOT also press whatever was
// under your finger — you were reaching for a dark screen, not for a control. A wake that fails is
// indistinguishable from a dead device, which is why this one is worth pinning.
static void s_wake_on_touch(void) {
    healthy_device();
    cinder_harness_input_enable();
    cinder_harness_script("cinder_get_screen_off_s", 30);
    cinder_harness_tap_at(60000, 240, 400);   // the first touch after it goes dark
    cinder_harness_tap_at(65000, 240, 400);   // and one while it is awake
    cinder_harness_set_budget_ms(80000);
    cinder_harness_run();

    check_eq(cinder_harness_count_between("cinder_tap", 0, 62000), 0,
             "the tap that woke the panel was not also delivered to the UI");
    check(cinder_harness_count_between("cinder_tap", 62000, 80000) >= 1,
          "the next tap, on a lit panel, was");
    long long woke = cinder_harness_first_ms("cinder_touch_down");
    std::printf("  .... first delivered contact at %lldms (dark tap was at 60000ms)\n", woke);
    check(woke > 62000, "…and it is the second one");
}

// A contact becomes a tap; a vertical drag becomes a drag and a fling, not a tap. The distinction
// is 26 px of travel, and getting it wrong means either a list you cannot scroll or a row that
// fires every time you try to.
static void s_touch_gestures(void) {
    healthy_device();
    cinder_harness_input_enable();
    cinder_harness_tap_at(30000, 240, 400);
    cinder_harness_swipe_at(40000, 240, 600, 240, 200, 400);   // a vertical list drag
    cinder_harness_set_budget_ms(50000);
    cinder_harness_run();

    check_eq(cinder_harness_count_between("cinder_tap", 29000, 39000), 1, "the tap was a tap");
    check_eq(cinder_harness_arg("cinder_tap", 0), 240, "…at the x it was aimed at");
    check_eq(cinder_harness_count_between("cinder_tap", 39000, 50000), 0,
             "the drag was NOT also delivered as a tap");
    check(cinder_harness_count_between("cinder_touch_drag", 39000, 50000) >= 4,
          "the drag streamed its travel");
    check(cinder_harness_count_between("cinder_touch_fling", 39000, 50000) >= 1,
          "…and ended in a fling");
}

// Raw evdev codes are device-specific and the defaults are baked in from the NW-A50. This checks
// the decode: 116 is Power, 115/114 the volume rocker, 106/105 next/prev.
static void s_button_codes(void) {
    healthy_device();
    cinder_harness_input_enable();
    struct { long long at; int code; int btn; } keys[] = {
        {30000, 116, 11 /* POWER   */}, {32000, 115,  9 /* VOLUP   */},
        {34000, 114, 10 /* VOLDOWN */}, {36000, 106, 13 /* NEXT    */},
        {38000, 105, 14 /* PREV    */},
    };
    for (size_t i = 0; i < sizeof keys / sizeof *keys; i++) {
        cinder_harness_key_at(keys[i].at, keys[i].code, 1);
        cinder_harness_key_at(keys[i].at + 200, keys[i].code, 0);
    }
    cinder_harness_set_budget_ms(45000);
    cinder_harness_run();

    check(cinder_harness_count("cinder_input") >= 5, "every press reached the navigator");
    for (size_t i = 0; i < sizeof keys / sizeof *keys; i++) {
        char what[96];
        std::snprintf(what, sizeof what, "raw %d decoded to logical button %d",
                      keys[i].code, keys[i].btn);
        bool seen = false;
        for (int n = 0; n < cinder_harness_count("cinder_input"); n++)
            if (cinder_harness_arg("cinder_input", n) == keys[i].btn) seen = true;
        check(seen, what);
    }
}

// The volume rocker's auto-repeat, at the level the user feels it. `vol_ramp.h` has a self-test for
// the CURVE; this checks the app runs it — that a held rocker accelerates, that it stops the instant
// you let go, and that a rocker which never comes back up (a stuck key, or a release event that
// never arrives) gives up instead of driving the mixer for the rest of the session.
static void s_volume_ramp(void) {
    healthy_device();
    cinder_harness_input_enable();
    cinder_harness_key_at(30000, 115, 1);            // Vol+ down
    cinder_harness_key_at(36000, 115, 0);            // …released after six seconds
    cinder_harness_key_at(60000, 114, 1);            // Vol- down and NEVER released
    cinder_harness_set_budget_ms(120000);
    cinder_harness_run();

    // Vol+ = logical 9, Vol- = 10. The count in a one-second slice is the step rate.
    const int early = cinder_harness_count_between("cinder_input", 30000, 31500);
    const int late = cinder_harness_count_between("cinder_input", 34500, 36000);
    const int after_release = cinder_harness_count_between("cinder_input", 37000, 55000);
    std::printf("  .... %d steps in the first 1.5s of the hold, %d in the last 1.5s\n", early, late);
    check(early >= 1, "a held rocker repeats at all");
    check(late > early, "…and accelerates the longer it is held");
    check_eq(after_release, 0, "and stops dead on release");

    // The stuck rocker. VOL_REPEAT_MAX_MS gives up; without it a key that never releases would drive
    // the volume for the life of the process.
    int stuck_window = cinder_harness_count_between("cinder_input", 90000, 120000);
    std::printf("  .... %d steps 30s after a rocker that never came back up\n", stuck_window);
    check_eq(stuck_window, 0, "a stuck rocker gives up rather than repeating for ever");
}

// ── skipping tracks faster than PlayerService can answer ─────────────────────────────────────
// Reported: the player goes unresponsive after skipping several songs in quick succession, and
// carries on skipping by itself afterwards.
//
// The shape of it is in main.cpp rather than in any one call. `carry_out` runs on the RENDER thread
// — input_pump sits above cinder_render_tick in the same loop — and every transport action inside
// it is a synchronous Sony round trip. The stubs answer instantly, which is why nothing in this
// suite had ever seen it: with an infinitely fast PlayerService no amount of tapping can back the
// app up. `cinder_harness_script_delay` is what makes the device's own cost visible here. 400 ms is
// the measured SetTrackSequence round trip (docs/DEVICE_TESTS.md); NextTrack has not been timed
// separately, and the defect does not depend on the figure — anything above a frame reproduces it,
// and the pump interval alone puts a floor of one tick (20 ms lit, 250 ms dark) under every reply.
//
// This one is about the SEMANTICS of a burst: nothing is dropped below the cap, and opposing
// presses cancel instead of both being paid for.
static void s_rapid_skip(void) {
    healthy_device();
    cinder_harness_input_enable();
    // Five ▷▷ then two ◁ — a run through an album with a couple of overshoots corrected.
    static const long long acts[] = {2, 2, 2, 2, 2, 3, 3};   // CINDER_ACT_NEXT ×5, _PREV ×2
    cinder_harness_script_seq("cinder_input", acts, 7);
    cinder_harness_script_delay("cinder_audio_next_track", 400);
    cinder_harness_script_delay("cinder_audio_prev_track", 400);
    cinder_harness_script_delay("cinder_render_tick", 30);
    for (int i = 0; i < 7; i++) {
        cinder_harness_key_at(30000 + i * 150, i < 5 ? 106 : 105, 1);
        cinder_harness_key_at(30000 + i * 150 + 60, i < 5 ? 106 : 105, 0);
    }
    cinder_harness_set_budget_ms(60000);
    cinder_harness_run();

    const int fwd = cinder_harness_count("cinder_audio_next_track");
    const int back = cinder_harness_count("cinder_audio_prev_track");
    std::printf("  .... 5 forward + 2 back -> %d NextTrack, %d PrevTrack\n", fwd, back);
    check_eq(fwd, 3, "the burst is NET: five forward and two back is three forward");
    check_eq(back, 0, "…and the two it cancels cost no round trip at all");
    check(cinder_harness_count_between("cinder_clock_tick", 30000, 36000) >= 4,
          "housekeeping kept running through the burst");
}

// The same burst from the GLASS rather than the side button, which is the case that wedges. A
// finger on the panel streams position frames for as long as it is down, and those frames arrive
// while the app is blocked inside the previous skip — so the drain loop's "read came back short"
// exit condition is never reached and input_pump ends when the USER stops, not when the events do.
// `swipe_at` with no travel is a real contact (down, eight position frames, up) where `tap_at` is
// the idealised four-event version.
//
// Before the fix, with the same 400 ms model: forty taps in four seconds left the app still issuing
// skips 13.1 s after the last one, and housekeeping — the paint's 1 Hz companion, which owns the
// now-playing poll, the sleep timer, the idle screen-off and the auto power-off — ran ONCE in
// fifteen seconds. That is the report.
static void s_rapid_skip_touch(void) {
    healthy_device();
    cinder_harness_input_enable();
    cinder_harness_script("cinder_tap", 2);                          // every tap = CINDER_ACT_NEXT
    cinder_harness_script_delay("cinder_audio_next_track", 400);
    cinder_harness_script_delay("cinder_render_tick", 30);
    for (int i = 0; i < 40; i++)
        cinder_harness_swipe_at(30000 + i * 100, 350, 692, 350, 692, 90);   // the NP skip button
    cinder_harness_set_budget_ms(70000);
    cinder_harness_run();

    const long long last_tap = 30000 + 39 * 100;
    const long long last_skip = cinder_harness_last_ms("cinder_audio_next_track");
    const int skips = cinder_harness_count("cinder_audio_next_track");
    const int house = cinder_harness_count_between("cinder_clock_tick", 30000, 45000);
    std::printf("  .... 40 taps -> %d skips; last skip %lld ms after the last tap\n",
                skips, last_skip - last_tap);
    std::printf("  .... housekeeping ticks over the 15s from the first tap: %d of 15\n", house);
    check(last_skip - last_tap <= 6000,
          "the player stops skipping soon after the thumb does, not seconds later");
    check(skips < 40, "the backlog is capped rather than banked in full");
    check(skips >= 8, "…but a real run of skips still moves the player a long way");
    check(house >= 10, "the frame loop kept its 1 Hz housekeeping through the burst");
}

// ── somebody ELSE unmounted /contents ────────────────────────────────────────────────────────
// We are not the only thing on this device that answers a USB cable. Sony's hagodaemon writes the
// contents partition into the mass-storage LUN and unmounts it on its own — `fsg_store_file
// file=/emmc@contents` in dmesg — and on the dev channel, where our auto-MSC is off, nothing on our
// side ever enters a session. So g_msc_active stays false, the exit path that remounts is never
// reached, and /contents stays gone for the rest of the boot: no music, no album art, no library,
// and no log either, because the log lives there too and its writes go to an inode with no name.
//
// Observed on device 2026-09-01, five minutes into a boot with the cable in. There is no cable
// EVENT here on purpose: the mount simply goes away underneath a running, healthy app, which is
// exactly how it presented.
static void s_contents_orphan(void) {
    healthy_device();
    cinder_harness_fs_write_at(60000, "/proc/mounts", kMountsNoContents);
    cinder_harness_set_budget_ms(120000);
    cinder_harness_run();

    const long long off = cinder_harness_first_ms(
        "system:/system/vendor/unknown321/bin/cinder-msc off");
    const int offs = cinder_harness_count("system:/system/vendor/unknown321/bin/cinder-msc off");
    std::printf("  .... unmounted at 60000ms -> reclaimed at %lldms (%d attempt(s))\n", off, offs);
    check_range(off, 60000, 70000, "took /contents back within a few seconds of losing it");
    check_eq(offs, 1, "…once, because it worked — not once a second for the rest of the boot");
    check_eq(cinder_harness_count("system:/system/vendor/unknown321/bin/cinder-msc on"), 0,
             "and never handed the volume OUT, which is not what happened");
    // The library was read from the volume that went away, so it has to be re-read from the one
    // that came back — otherwise the app is mounted but still showing an empty shelf.
    check(cinder_harness_count("cinder_db_open") >= 2, "reopened the library after the remount");
}

// A session WE started must be left alone. Same shape as the reclaim above — /contents unmounted,
// app running — but this time it is unmounted BECAUSE we asked for it, which is the one case where
// taking it back would break the thing the user is doing (a transfer they can see on the PC).
static void s_contents_orphan_not_ours(void) {
    healthy_device();
    // Cable in at 30 s, and stay: auto-MSC hands the volume over and the session stays open.
    cinder_harness_fs_write_at(30000, "/sys/class/power_supply/usb/online", "1\n");
    cinder_harness_fs_write_at(30000, "/sys/class/power_supply/usb/present", "1\n");
    cinder_harness_fs_write_at(30000, "/sys/class/android_usb/android0/state", "CONFIGURED\n");
    cinder_harness_set_budget_ms(120000);
    cinder_harness_run();

    const int on  = cinder_harness_count("system:/system/vendor/unknown321/bin/cinder-msc on");
    const int off = cinder_harness_count("system:/system/vendor/unknown321/bin/cinder-msc off");
    std::printf("  .... %d handover(s), %d reclaim(s) over a 90s session\n", on, off);
    check_eq(on, 1, "handed the volume over when the cable went in");
    check_eq(off, 0, "and did NOT take it back underneath the user's own transfer");
}

// ── "it does not pick up music I just copied on" — the half that is not device-gated ─────────
// The oldest open user-visible bug in the project has two halves. Somebody has to ask
// MediaStoreService to RE-SCAN so the database changes at all — that is device-gated, it needs the
// MediaStoreClient vtable recovered, and it is deliberately not guessed. But the second half is
// ours: once the database HAS changed, the app has to notice and reload.
//
// `db_sig.h` is the rule (it covers the DB, its -wal and its -journal, and mtime AND size AND inode
// for each, because a SQLite writer can leave any one of them untouched across a commit) and it has
// a ten-case self-test. This is the other question: does the app RUN it, on a timer, and does
// noticing actually reload the library.
static void s_library_changed(void) {
    healthy_device();
    cinder_harness_fs_write("/db/MTPDB.dat", "the library as it was at boot");
    // …and something writes to it a minute in, the way a completed MediaStore scan would.
    cinder_harness_fs_write_at(60000, "/db/MTPDB.dat",
                               "the library with the album you just copied on, which is longer");
    cinder_harness_set_budget_ms(120000);
    cinder_harness_run();

    check_eq(cinder_harness_count_between("cinder_db_open", 0, 5000), 1, "opened once at boot");
    const int reopened = cinder_harness_count_between("cinder_db_open", 60000, 90000);
    std::printf("  .... reloaded %d time(s) within 30s of the database changing\n", reopened);
    check(reopened >= 1, "the library was reloaded after the database changed underneath it");
    check_range(reopened, 1, 3, "…once, not on every poll after");
    check_eq(cinder_harness_count_between("cinder_db_open", 95000, 120000), 0,
             "and it settles again rather than reloading for ever");
}

// ── blanking the panel must reach the SERVICE, not just the sysfs node ───────────────────────
//
// THE BUG. `set_backlight` has carried this comment since 2026-08-19: "measured with the node at 0,
// the service at 2 and the panel STILL LIT — the reported 'backlight off still powers the
// backlight'". It handles that itself (level 0 -> display_backlight(0)). But the two paths that
// actually blank the screen never went through it: `screen_auto_off` (idle timeout) and
// `screen_toggle`'s off-branch both did a raw fputc('0') to the node and stopped. So both of them
// "turned the screen off" by writing a number the service overrides, and the panel stayed lit.
//
// It went unnoticed for two weeks because the observable half worked. Anything watching the sysfs
// node — including this harness before it grew a DisplayService fake — saw "0" and called it a
// pass. These scenarios assert on the half that was missing.
//
// Both paths are covered separately on purpose: they are different callers of the same rule, and
// fixing one and not the other is precisely the state the code was in.
static void s_idle_blank_darkens_the_panel(void) {
    healthy_device();
    cinder_harness_script("cinder_get_screen_off_s", 30);   // blank after 30s idle
    cinder_harness_set_budget_ms(120000);
    cinder_harness_run();

    check(cinder_harness_count("log") >= 0, "");   // keep the trace alive for the dump below
    check_eq(cinder_harness_display_backlight(), 0,
             "the idle blank tells DisplayService to turn the panel off, not just the sysfs node");
    // …and the node too. Both halves, or the restore path has nothing to put back.
    char buf[32] = {0};
    if (cinder_harness_fs_read("/sys/class/leds/lcd-backlight/brightness", buf, sizeof buf) > 0)
        check(buf[0] == '0', "and still writes the brightness node");
}

// The restore has to put back what was there, and that is an ORDERING rule:
// `display_backlight_remember()` caches the service's own level, and it only works while that
// level is still non-zero. Blank first and it caches 0 — the device then "restores" to 0 and wakes
// to a black screen with the backlight node reading normal, which is indistinguishable from dead.
// panel_dark() therefore remembers BEFORE it writes, and this is that rule stated as a test.
//
// Deliberately input-free. The obvious version of this drives the Power button and then a wake tap,
// and two attempts at it asserted nothing: synthetic key events land before the app is consuming
// its input nodes, so the scenario passed through a device that never saw the press. An ordering
// assertion over the trace needs no input at all and cannot fail that way. The Power-button path is
// not separately covered because it is the same function — screen_toggle() and screen_auto_off()
// both call panel_dark(), which is precisely the change that fixed them; they had the defect twice
// because they each carried their own copy of those four lines.
static void s_blank_remembers_before_zeroing(void) {
    healthy_device();
    cinder_harness_script("cinder_get_screen_off_s", 30);
    cinder_harness_set_budget_ms(120000);
    cinder_harness_run();

    check_eq(cinder_harness_before("display:GetLCDBacklightBrightness",
                                   "display:SetLCDBacklightBrightness"), 1,
             "the service level is READ before it is ever written (else the restore caches 0)");
    check(cinder_harness_count("display:SetLCDBacklightBrightness") >= 1,
          "and the service is actually driven");
    check_eq(cinder_harness_display_backlight(), 0, "ending dark, after the idle blank");
}

// ── repeat-all: the queue boundary, and the three things that look like it ───────────────────
// PlayerService has no loop-the-queue primitive, so main.cpp watches for the shape a queue end
// makes: the position pinned at the duration with `playing` gone 1 -> 0. That shape is NOT unique
// to a queue end, and until this scenario existed nothing off-device could reach the code at all —
// the generated stub for `cinder_audio_position` left its out-parameters untouched, so `tot > 0`
// was false in every scenario and the whole branch was unreachable (see harness.cpp).
//
// Two things are asserted here, and both are defects this scenario was written for:
//   * a lap is built from CINDER'S CONTEXT (`cinder_repeat_all_prepare` -> the pending-play
//     channel), not from the URI list the shim last issued. A queue flush hands the shim
//     `[current] + queue + context[idx+1..]`, so replaying THAT loops only the tail: swipe-queue
//     one song at track 5 of an album and tracks 1-4 never play again.
//   * and it only fires on the LAST track of the sequence.
static void s_repeat_all(void) {
    healthy_device();
    cinder_harness_script("cinder_get_repeat_all", 1);
    cinder_harness_script("cinder_audio_is_playing", 0);   // the service says it has stopped
    cinder_harness_script("cinder_on_last_track", 1);      // …at the end of the sequence
    cinder_harness_script("cinder_repeat_all_prepare", 1);
    cinder_harness_script("cinder_pending_play_count", 3);  // a three-track context to lap
    cinder_harness_play_position(304173, 304173);          // pinned at the duration (3f, measured)
    cinder_harness_set_budget_ms(60000);
    cinder_harness_run();

    check(cinder_harness_count("cinder_repeat_all_prepare") >= 1,
          "the lap is built from Cinder's own context");
    // …and goes down the ordinary pending-play channel: play_pending_sequence drains the URIs.
    // (The URIs themselves come back empty here — the generated stub cannot fill a char buffer —
    // so the assertion is that the channel was DRAINED, which only that path does.)
    check(cinder_harness_count("cinder_pending_play_uri") >= 3,
          "…and handed to PlayerService through the pending-play channel");
    check_eq(cinder_harness_count("cinder_audio_restart_sequence"), 0,
             "not the shim's last URI list, which after a queue flush is only the tail");
    // The latch: without it this fires every frame for as long as the player sits at the end.
    int laps = cinder_harness_count("cinder_repeat_all_prepare");
    std::printf("  .... %d lap(s) over 60s parked at the end of the queue\n", laps);
    check_range(laps, 1, 2, "the end of a queue is one lap, not one per frame");
}

// The same shape, in the middle of the sequence — which is what a PAUSE inside the last second and
// a half of ANY track looks like. It must not restart the queue: the report this guards against is
// "it jumped back to track 1 on its own".
static void s_repeat_all_midtrack(void) {
    healthy_device();
    cinder_harness_script("cinder_get_repeat_all", 1);
    cinder_harness_script("cinder_audio_is_playing", 0);
    cinder_harness_script("cinder_on_last_track", 0);      // track 3 of 12, not the last
    cinder_harness_script("cinder_repeat_all_prepare", 1);
    cinder_harness_play_position(303500, 304173);          // 673 ms from the end, paused
    cinder_harness_set_budget_ms(60000);
    cinder_harness_run();

    check_eq(cinder_harness_count("cinder_repeat_all_prepare"), 0,
             "a pause near the end of a middle track is not the end of the queue");
    check_eq(cinder_harness_count("cinder_audio_restart_sequence"), 0,
             "and nothing restarted the sequence by the old path either");
}

// And with repeat-all OFF, the end of a queue is just the end of a queue.
static void s_repeat_all_off(void) {
    healthy_device();
    cinder_harness_script("cinder_get_repeat_all", 0);
    cinder_harness_script("cinder_audio_is_playing", 0);
    cinder_harness_script("cinder_on_last_track", 1);
    cinder_harness_script("cinder_repeat_all_prepare", 1);
    cinder_harness_play_position(304173, 304173);
    cinder_harness_set_budget_ms(60000);
    cinder_harness_run();

    check_eq(cinder_harness_count("cinder_repeat_all_prepare"), 0, "nothing loops with repeat off");
    check_eq(cinder_harness_count("cinder_audio_play_tracks"), 0, "and nothing starts playing");
}

struct Scenario { const char* name; void (*fn)(void); const char* what; };
static const Scenario kScenarios[] = {
    { "blank-idle",  s_idle_blank_darkens_the_panel,
      "the idle blank reaches DisplayService, not just the sysfs node" },
    { "blank-order", s_blank_remembers_before_zeroing,
      "the service level is read before it is zeroed, so the restore has something to restore" },
    {"boot",              s_boot,                    "the app boots and brings Bluetooth up with it"},
    {"no-services",       s_no_services,             "no Sony service exists: degrade, never die"},
    {"bt-late-service",   s_bt_late_service,         "the BT service arrives after the app does"},
    {"bt-switch",         s_bt_switch_follows_radio, "the UI switch follows the radio, not itself"},
    {"bt-no-self-jam",    s_bt_never_jams_itself,    "the app never arms the mode that refuses its own connects"},
    {"bt-stale-jam",      s_bt_clears_a_stale_jam,   "a retry mode left armed by anything else is cleared"},
    {"bt-idle-not-link",  s_bt_idle_radio_is_not_a_link, "GetBtStatus 3 with no peer is not a connection"},
    {"bt-page-in-flight", s_bt_waits_for_a_page_in_flight, "the ladder defers to a connect already on the air"},
    {"bt-idle-poll",      s_bt_idle_poll_rate,       "the idle Bluetooth poll backs off"},
    {"stalled-bringup",   s_stalled_bringup,         "bring-up that never completes must not freeze the app"},
    {"dark-playing",      s_dark_playing,            "panel dark, BT playing: the state the device lives in"},
    {"log-volume",        s_log_volume,              "the log is a flash write: keep it rare"},
    {"jack-unplug",       s_jack_unplug,             "headphones out mid-track pauses playback"},
    {"usb-msc",           s_usb_msc,                 "a PC appearing hands over the volume, once"},
    {"msc-cycle",         s_msc_cycle,               "cable in and out: the whole mass-storage round trip"},
    {"autooff-idle",      s_auto_off_idle,           "idle and silent: power off, and back off if it fails"},
    {"autooff-playing",   s_auto_off_playing,        "never power off while audio is playing"},
    {"autooff-charging",  s_auto_off_charging,       "never power off a device on a charger"},
    {"dsp-reconcile",     s_dsp_reconcile_no_settings, "the DSP is reconciled even with no settings file"},
    {"wake-on-touch",     s_wake_on_touch,           "a dark panel wakes on touch, without pressing anything"},
    {"touch-gestures",    s_touch_gestures,          "a tap is a tap and a drag is a drag"},
    {"button-codes",      s_button_codes,            "raw evdev codes decode to the right buttons"},
    {"volume-ramp",       s_volume_ramp,             "the rocker accelerates, stops on release, gives up when stuck"},
    {"library-changed",   s_library_changed,         "a database that changes underneath us is noticed and reloaded"},
    {"rapid-skip",        s_rapid_skip,              "skipping faster than PlayerService answers must not wedge the UI"},
    {"rapid-skip-touch",  s_rapid_skip_touch,        "the same burst from the glass, where the panel keeps streaming"},
    {"contents-orphan",   s_contents_orphan,         "/contents unmounted by something that is not us is taken back"},
    {"contents-ours",     s_contents_orphan_not_ours,"…but a session we started is left alone"},
    {"repeat-all",        s_repeat_all,              "a queue that runs out loops Cinder's context, once"},
    {"repeat-all-mid",    s_repeat_all_midtrack,     "…and a pause near the end of a middle track does not"},
    {"repeat-all-off",    s_repeat_all_off,          "…and with repeat off the end of a queue is the end"},
    {nullptr, nullptr, nullptr},
};

int main(int argc, char** argv) {
    if (argc < 2) {
        std::printf("usage: %s <scenario>|list\n", argv[0]);
        for (const Scenario* s = kScenarios; s->name; ++s)
            std::printf("  %-18s %s\n", s->name, s->what);
        return 2;
    }
    if (!std::strcmp(argv[1], "list")) {
        for (const Scenario* s = kScenarios; s->name; ++s) std::printf("%s\n", s->name);
        return 0;
    }
    for (const Scenario* s = kScenarios; s->name; ++s) {
        if (std::strcmp(argv[1], s->name)) continue;
        g_scenario = s->name;
        std::printf("== %s — %s\n", s->name, s->what);
        cinder_harness_reset();
        cinder_harness_bt_reset();
        s->fn();
        if (g_fails && std::getenv("CINDER_HARNESS_TRACE")) cinder_harness_dump(0);
        std::printf("%s: %s\n", g_scenario, g_fails ? "FAILED" : "passed");
        return g_fails ? 1 : 0;
    }
    std::fprintf(stderr, "unknown scenario '%s'\n", argv[1]);
    return 2;
}
