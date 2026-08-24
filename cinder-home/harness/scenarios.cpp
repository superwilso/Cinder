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
static void healthy_device(void) {
    cinder_harness_script("cinder_frames_presented", 1);
    cinder_harness_script("cinder_render_init", 0);
    cinder_harness_script("cinder_db_open", 0);
    cinder_harness_script("cinder_audio_init", 0);
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
    cinder_harness_set_budget_ms(120000);
    cinder_harness_run();

    check(cinder_harness_count("cinder_render_tick") > 100, "still painting");
    check(cinder_harness_count_between("cinder_db_open", 60000, 120000) >= 30,
          "still retrying the DB, paced at ~1 Hz");

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
}

struct Scenario { const char* name; void (*fn)(void); const char* what; };
static const Scenario kScenarios[] = {
    {"boot",              s_boot,                    "the app boots and brings Bluetooth up with it"},
    {"no-services",       s_no_services,             "no Sony service exists: degrade, never die"},
    {"bt-late-service",   s_bt_late_service,         "the BT service arrives after the app does"},
    {"bt-switch",         s_bt_switch_follows_radio, "the UI switch follows the radio, not itself"},
    {"bt-idle-poll",      s_bt_idle_poll_rate,       "the idle Bluetooth poll backs off"},
    {"stalled-bringup",   s_stalled_bringup,         "bring-up that never completes must not freeze the app"},
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
