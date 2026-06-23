// easel_abi.hpp — hand-written declarations for Sony's (undocumented) easel app
// framework, reconstructed by RE (analysis/F_appmgr_home/). We have no SDK headers,
// so these declarations exist only to make our calls link against the device .so's
// with the correct C++/libc++ mangling. Layout/signatures are from Ghidra + nm.
//
// ABI REQUIREMENT: build with clang -stdlib=libc++ to match Sony's libc++ (the
// symbols use std::__1::function / std::__1::unique_ptr). g++/libstdc++ will NOT
// interoperate.  See build.sh.
#pragma once
#include <functional>
#include <memory>
#include <string>

namespace easel {

// ModuleBaseInterface — the lifecycle-driven unit registered with the app. Its vtable
// is driven by LifeCycleManager (slots: +8 OnInitialize, +0xc OnPostInitialize,
// +0x10 OnActivate, +0x14 OnForeground, ...). We never subclass it directly — the
// CUI module below provides a concrete one driven by std::function callbacks.
class ModuleBaseInterface {
public:
    virtual ~ModuleBaseInterface();
};

// ApplicationBase — the app object. run() builds the AppManagerModule (which connects
// to `appmgrservice` and performs the Foreground handshake), registers the modules,
// and runs the lifecycle pump.  vtable (confirmed): [0],[1] = TWO PURE VIRTUALS that a
// concrete app MUST implement (identity TODO — see README "Open items"); [2]=OnInitialize
// … [7]=OnForeground … through StopResumeAnimation.
class ApplicationBase {
public:
    ApplicationBase();
    virtual ~ApplicationBase();

    // The one-call bootstrap. `module` is the app's UI module (we pass a CuiAppModule).
    void run(int argc, char** argv, const char* name,
             std::unique_ptr<ModuleBaseInterface> module);

    void  SetPumpTriggerHandler(std::function<void()> trigger);
    void* GetAppParam();

    // RESOLVED (RE of HgrmMediaPlayerApp's concrete vtable): vtable slots 0,1 are NOT two
    // mystery abstract methods — they are the class's **pure virtual destructor**
    // (`virtual ~ApplicationBase() = 0`), which occupies the D1/D0 slots. A concrete subclass
    // satisfies it simply by having a destructor (compiler-generated is fine). No extra
    // methods to implement. (`virtual ~ApplicationBase()` above maps to those slots.)

    // lifecycle virtuals (have safe default impls in ApplicationBase) — override as needed.
    // ORDER MUST MATCH the device vtable (confirmed): ~dtor, OnInitialize, OnPostInitialize,
    // OnActivate, OnForeground, OnBackground, OnInactivate, OnFinalize, OnSuspend, OnResume, …
    virtual void OnInitialize();
    virtual void OnPostInitialize();
    virtual void OnActivate();
    virtual void OnForeground();
    virtual void OnBackground();
    virtual void OnInactivate();
    virtual void OnFinalize();
};

// CuiAppModule — the non-Qt (framebuffer) module. Constructor (confirmed by demangle):
//   CuiAppModule(ApplicationBase&, int argc, char** argv,
//                function<void()> onInitialize,
//                function<void()> onPostInitialize,
//                function<void()> onForeground,     // (order of the 5 void cbs TBC on device)
//                function<void()> onBackground,
//                function<void()> onFinalize,
//                function<bool()> onPumpTrigger,     // render tick; return true to keep pumping
//                function<void()> onActivate)
// It implements ModuleBaseInterface and dispatches each lifecycle step to the callback.
class CuiAppModule : public ModuleBaseInterface {
public:
    CuiAppModule(ApplicationBase& app, int argc, char** argv,
                 std::function<void()> cb1,
                 std::function<void()> cb2,
                 std::function<void()> cb3,
                 std::function<void()> cb4,
                 std::function<void()> cb5,
                 std::function<bool()> pump,
                 std::function<void()> cb7);
};

} // namespace easel
