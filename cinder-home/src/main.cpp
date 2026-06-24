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
// The playback-control shim over Sony's PlayerService (cinder-audio/player_shim.cpp).
#include "cinder_audio.h"

namespace {

// Carry out a navigator Action (returned by cinder_input) via PlayerService. This is the
// bridge that keeps the UI (Rust) free of audio concerns: the navigator decides *what*,
// the shell does it here. (PLAY_INDEX/VOL need a bit more plumbing — see TODOs.)
void dispatch_action(int act) {
    switch (act) {
        case CINDER_ACT_PLAYPAUSE:   /* toggle: poll state then play/pause */ break;
        case CINDER_ACT_NEXT:        cinder_audio_next_track(); break;
        case CINDER_ACT_PREV:        cinder_audio_prev_track(); break;
        case CINDER_ACT_NEXT_ALBUM:  cinder_audio_next_group(); break;
        case CINDER_ACT_PREV_ALBUM:  cinder_audio_prev_group(); break;
        case CINDER_ACT_ENTER_USB_MSC: /* shell triggers mass storage (see RECOVERY.md) */ break;
        case CINDER_ACT_SLEEP:       /* shell drives suspend */ break;
        default: break; // NONE / PLAY_INDEX / VOL* — TODO plumb index + volume service
    }
}

// Concrete app. ApplicationBase's "two pure virtuals" (vtable slots 0,1) are RESOLVED:
// they are its pure virtual DESTRUCTOR, satisfied below by ~CinderApp(). The header
// declares all 20 vtable slots in device order, so this subclass's vtable matches the
// framework's expectations (mis-sized/reordered vtable = wrong-slot dispatch = reboot).
class CinderApp : public easel::ApplicationBase {
public:
    ~CinderApp() override = default;   // satisfies ApplicationBase's pure virtual destructor
    void OnForeground() override {
        // We are now the foreground Home app — appmgr's WaitLifeCycleChanged(state=1) is
        // satisfied by the framework, so no reboot. Bring up renderer + library + audio.
        if (cinder_render_init() != 0) {
            std::fprintf(stderr, "cinder-home: render init failed\n");
        }
        cinder_db_open("/db/MTPDB.dat");   // library reader (path: confirm on device)
        cinder_audio_init("cinder");       // PlayerService control (poll mode)
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
    auto pump = []() -> bool {
        // TODO(device): read /dev/input/hoge events here and, for each logical button,
        //   dispatch_action(cinder_input(button));
        // The raw evdev keycode -> cinder_button_t map needs on-device getevent calibration
        // (it isn't in any extracted DTB). Also poll PlayerService here and push the result:
        //   cinder_set_now_playing_uri(uri, progress, playing, battery);
        cinder_render_tick();
        return true;
    };

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
