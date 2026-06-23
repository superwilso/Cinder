// cinder-home — make Cinder a *valid* easel "Home" app so app-manager launches it
// instead of the stock Qt UI, completing the Foreground handshake so the device does
// NOT reboot (see analysis/F_appmgr_home/RE_findings.md for the protocol).
//
// Strategy: don't subclass a fragile vtable — use the non-Qt easel::CuiAppModule and
// hand it std::function callbacks. The module + run() perform the appmgr connect and
// the lifecycle ACKs; our only job is to start painting the framebuffer at OnForeground
// and tick the renderer on the pump.
//
// STATUS: BLUEPRINT. It encodes the confirmed API but is not yet buildable/runnable —
// two prerequisites remain (see README): (1) a clang+libc++ armhf toolchain matching the
// device's libc++ ABI; (2) the identity of ApplicationBase's 2 pure virtuals so a
// concrete app can be instantiated.
#include "easel_abi.hpp"
#include <memory>
#include <cstdio>

// The render core: the Rust Cinder UI, built as a glibc C-ABI staticlib
// (player/cinder-ffi -> libcinder_ffi.a). C ABI, so the renderer stays in Rust while
// this shell stays C++/libc++. See player/cinder-ffi/include/cinder.h.
#include "cinder.h"

namespace {

// Concrete app. NOTE: to compile, this must also override ApplicationBase's two pure
// virtuals (slots 0,1) — unresolved; see README "Open items".
class CinderApp : public easel::ApplicationBase {
public:
    ~CinderApp() override = default;   // satisfies ApplicationBase's pure virtual destructor
    void OnForeground() override {
        // We are now the foreground Home app — appmgr's WaitLifeCycleChanged(state=1) is
        // satisfied by the framework, so no reboot. Start the renderer.
        if (cinder_render_init() != 0) {
            std::fprintf(stderr, "cinder-home: render init failed\n");
        }
        easel::ApplicationBase::OnForeground();
    }
    void OnBackground() override {
        cinder_render_shutdown();
        easel::ApplicationBase::OnBackground();
    }
};

} // namespace

int main(int argc, char** argv) {
    CinderApp app;

    // Build the CUI module. The render tick goes in the pump callback (returns true to
    // keep pumping). The lifecycle void-callbacks are mostly no-ops here because we also
    // override the ApplicationBase hooks; either path works — we keep the renderer in
    // OnForeground/OnBackground and tick from the pump.
    auto noop = []() {};
    auto pump = []() -> bool { cinder_render_tick(); return true; };

    auto module = std::unique_ptr<easel::ModuleBaseInterface>(
        new easel::CuiAppModule(app, argc, argv,
            /*cb1*/ noop, /*cb2*/ noop, /*cb3*/ noop,
            /*cb4*/ noop, /*cb5*/ noop,
            /*pump*/ pump,
            /*cb7*/ noop));

    // Hand control to easel: connects to appmgrservice as the named app, drives the
    // lifecycle to Foreground (ACK within the timeout), runs the pump until shutdown.
    // The name must match the .appcfg `command:` slot we register the app under.
    app.run(argc, argv, "HgrmMediaPlayerApp", std::move(module));
    return 0;
}
