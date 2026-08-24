// fake_easel.cpp — a standing-in easel framework, so main.cpp can be BOOTED on a build machine.
//
// The real easel lives in libeaselcore.so / libeaselcui.so on the Walkman and there are no headers
// for it; `easel_abi.hpp` is a hand-recovered declaration of its ABI, and that same header is what
// this file implements. Two consequences worth knowing:
//   * The fake is bound by the same declarations the device build uses, so a change to the
//     recovered ABI breaks the harness build — which is the point. A silently-drifted vtable is
//     the failure mode that reboots the device.
//   * The fake DRIVES THE LIFECYCLE the way appmgr does (Initialize -> PostInitialize -> Activate
//     -> Foreground), because the app's whole bring-up sequence hangs off those hooks. Booting is
//     the test.
//
// One deliberate divergence from the device: on hardware, OnForeground never returns (the non-Qt
// CuiAppModule parks the main thread in a condition_variable nothing ticks — see main.cpp's
// render-driver note). Here it returns, and the harness then waits out the run budget on the main
// thread while the app's own render_driver worker does the work. The app cannot tell the
// difference: it never depended on OnForeground blocking, only on the worker existing.
#include "../src/easel_abi.hpp"
#include "harness.h"

#include <cstdio>
#include <unistd.h>

namespace {
easel::CuiAppModule* g_fake_module = nullptr;

// The 7 callbacks handed to the CuiAppModule ctor. The device object keeps them inside its own
// (0x100-byte) footprint; we keep them here rather than in the reserved storage, which we must not
// touch — its whole purpose is to be the size the device ctor would write into.
struct Callbacks {
    std::function<void()> init, post_init, activate, foreground, finalize, cb7;
    std::function<bool()> pump;
} g_cb;

long long g_budget_ms = 20000;
} // namespace

namespace easel {

ModuleBaseInterface::~ModuleBaseInterface() {}

ApplicationBase::ApplicationBase() {
    cinder_harness_record("easel:ApplicationBase()", 0);
}
ApplicationBase::~ApplicationBase() {}

// Default hook bodies. On the device these are real framework work; here they are trace points, so
// a test can assert the app got as far as Foreground (a boot that dies at Activate is the single
// most common way this app has failed on hardware).
void ApplicationBase::OnInitialize()          { cinder_harness_record("easel:OnInitialize", 0); }
void ApplicationBase::OnPostInitialize()      { cinder_harness_record("easel:OnPostInitialize", 0); }
void ApplicationBase::OnActivate()            { cinder_harness_record("easel:OnActivate", 0); }
void ApplicationBase::OnForeground()          { cinder_harness_record("easel:OnForeground", 0); }
void ApplicationBase::OnBackground()          { cinder_harness_record("easel:OnBackground", 0); }
void ApplicationBase::OnInactivate()          { cinder_harness_record("easel:OnInactivate", 0); }
void ApplicationBase::OnFinalize()            { cinder_harness_record("easel:OnFinalize", 0); }
void ApplicationBase::OnSuspend(bool&)        { cinder_harness_record("easel:OnSuspend", 0); }
void ApplicationBase::OnResume(const std::string&)     { cinder_harness_record("easel:OnResume", 0); }
void ApplicationBase::OnEarlySuspend(bool&)   { cinder_harness_record("easel:OnEarlySuspend", 0); }
void ApplicationBase::OnLateResume(const std::string&) { cinder_harness_record("easel:OnLateResume", 0); }
void ApplicationBase::OnPreShutdown(bool&)    { cinder_harness_record("easel:OnPreShutdown", 0); }
void ApplicationBase::OnPreResetSetting()     { cinder_harness_record("easel:OnPreResetSetting", 0); }
void ApplicationBase::OnResetSetting()        { cinder_harness_record("easel:OnResetSetting", 0); }
void ApplicationBase::OnPostResetSetting()    { cinder_harness_record("easel:OnPostResetSetting", 0); }
void ApplicationBase::StopBootAnimation()     { cinder_harness_record("easel:StopBootAnimation", 0); }
void ApplicationBase::StartResumeAnimation()  { cinder_harness_record("easel:StartResumeAnimation", 0); }
void ApplicationBase::StopResumeAnimation()   { cinder_harness_record("easel:StopResumeAnimation", 0); }

void ApplicationBase::run(int, char**, const char* name,
                          std::unique_ptr<ModuleBaseInterface> module) {
    cinder_harness_record("easel:run", 0);
    (void)name;

    // appmgr's order. Each phase runs the app's virtual hook and then the module callback of the
    // same name — that pairing is what the device does and what main.cpp is written against.
    OnInitialize();     if (g_cb.init)       g_cb.init();
    OnPostInitialize(); if (g_cb.post_init)  g_cb.post_init();
    OnActivate();       if (g_cb.activate)   g_cb.activate();
    OnForeground();     if (g_cb.foreground) g_cb.foreground();

    // From here the app's own render_driver worker owns the clock: it sleeps once per frame, and
    // that is what makes virtual time move. This thread must only ever WAIT for it, never set it,
    // or the frame loop's pacing would be measured against a clock this thread was running.
    cinder_harness_clock_never_owner();
    while (cinder_harness_now_ms() < g_budget_ms) sleep(1);

    if (g_cb.finalize) g_cb.finalize();   // stops + joins the worker (main.cpp's cbFinal)
    OnFinalize();
    module.reset();
}

CuiAppModule::CuiAppModule(ApplicationBase&, int, char**,
                           std::function<void()> onInitialize,
                           std::function<void()> onPostInitialize,
                           std::function<void()> onActivate,
                           std::function<void()> onForeground,
                           std::function<void()> onFinalize,
                           std::function<bool()> onPumpTrigger,
                           std::function<void()> cb7) {
    cinder_harness_record("easel:CuiAppModule()", 0);
    g_cb.init       = onInitialize;
    g_cb.post_init  = onPostInitialize;
    g_cb.activate   = onActivate;
    g_cb.foreground = onForeground;
    g_cb.finalize   = onFinalize;
    g_cb.pump       = onPumpTrigger;
    g_cb.cb7        = cb7;
    g_fake_module   = this;
}

CuiAppModule::~CuiAppModule() { g_fake_module = nullptr; }

// The app's ticker thread pokes this. On the device it notifies the module's condition_variable;
// here it is a trace point, and deliberately does NOT invoke the pump callback — main.cpp says the
// pump must stay inert because the worker owns the frame loop, and a harness that fired it anyway
// would be testing a configuration the device never runs.
void CuiAppModule::OnPumpTrigger() { cinder_harness_record("easel:OnPumpTrigger", 0); }

} // namespace easel

// main.cpp built with -Dmain=cinder_app_main. C++ linkage, like the main it was renamed from.
int cinder_app_main(int argc, char** argv);

extern "C" void cinder_harness_set_budget_ms(long long ms) { g_budget_ms = ms; }

extern "C" int cinder_harness_run(void) {
    char arg0[] = "cinder-home";
    char* argv[] = { arg0, nullptr };
    return cinder_app_main(1, argv);
}
