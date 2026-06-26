// easel_abi.hpp — hand-written declarations for Sony's (undocumented) easel app
// framework, reconstructed by RE (analysis/F_appmgr_home/). We have no SDK headers,
// so these declarations exist only to make our calls link against the device .so's
// with the correct C++/libc++ mangling AND to reproduce the EXACT vtable layout so a
// concrete subclass dispatches to the right slots.
//
// ABI REQUIREMENT: build with clang -stdlib=libc++ to match Sony's libc++ (the
// symbols use std::__1::function / std::__1::unique_ptr). g++/libstdc++ will NOT
// interoperate.  See build.sh.
//
// VTABLE GROUND-TRUTH (2026-06-24): extracted from libeaselcore.so
// _ZTVN5easel15ApplicationBaseE (.rel.dyn R_ARM_ABS32 entries, 20 slots). The order
// below MUST match it byte-for-byte — a shorter/reordered vtable in our subclass means
// the framework calls land in the wrong slot (the crash/reboot class of bug). Verified
// signatures via `nm -D | c++filt` on libeaselcore.so.
#pragma once
#include <functional>
#include <memory>
#include <string>
#include <cstddef>

namespace easel {

// ── CRITICAL: object SIZING (heap/stack overflow class of bug) ───────────────────────────
// These declarations have NO data members, so `sizeof` is just the vptr (4 bytes). But the
// REAL device ctors (in libeaselcore/libeaselcui) write the REAL object layout — far past 4
// bytes. `new easel::CuiAppModule(...)` would `operator new(4)` then the device ctor scribbles
// ~0x100 bytes, corrupting the heap; `CinderApp app;` on the stack would let the device
// ApplicationBase ctor write past the 4-byte stack slot. On 2026-06-25 this manifested as a
// SIGSEGV at CuiAppModule::OnInitialize+0x45 reading `[this+0x18]=0x12` (malloc metadata in the
// clobbered callback table). FIX: reserve storage sized from the ctor's HIGHEST WRITE OFFSET
// (read out of the Ghidra decompiles) so the allocation/stack slot is big enough; the device
// ctor owns those bytes, we never touch them. Sizes are deliberately generous (the object is
// short-lived and freed via the device's virtual deleting dtor, which glibc frees by chunk
// header regardless of our size). Update these if a future fw revision grows the objects.
//   easel::ApplicationBase : real = 8 bytes  (ctor @0x13e38 writes this+0 vptr, this+4 LCM*)
//   easel::CuiAppModule    : real ≈ 0x100     (ctor @0x11e60 writes through this[0xfa])
constexpr std::size_t kApplicationBaseRealSize = 8;     // ctor @0x13e38 writes this+0, this+4
constexpr std::size_t kCuiAppModuleRealSize    = 0x100; // ctor @0x11e60 writes through this[0xfa]

// ModuleBaseInterface — the lifecycle-driven unit registered with the app. We never
// subclass it directly; CuiAppModule (below, defined in libeaselcui) is the concrete one.
class ModuleBaseInterface {
public:
    virtual ~ModuleBaseInterface();
};

// ApplicationBase — the app object. It is ABSTRACT: vtable slots 0,1 are a PURE VIRTUAL
// DESTRUCTOR (both point to __cxa_pure_virtual in the base's own vtable). A concrete app
// satisfies it simply by existing with a destructor (compiler-generated is fine) — that
// fills slots 0,1 in the subclass vtable. ApplicationBase provides DEFAULT bodies for all
// the On*/animation virtuals (defined symbols in libeaselcore), so we only override what
// we need (OnForeground) and inherit the rest; declaring them here just reproduces the
// 20-slot layout so the subclass vtable is the right length and order.
class ApplicationBase {
public:
    ApplicationBase();
    virtual ~ApplicationBase() = 0; // slots 0,1 — base def is in libeaselcore (D2/D1)

    // --- non-virtual members (called directly by symbol, not via vtable) ---
    // The one-call bootstrap. `module` is the app's UI module (we pass a CuiAppModule).
    void  run(int argc, char** argv, const char* name,
              std::unique_ptr<ModuleBaseInterface> module);
    void  SetPumpTriggerHandler(std::function<void()> trigger);
    void* GetAppParam();
    void  Exit();

    // --- virtual lifecycle hooks, IN EXACT VTABLE ORDER (slots 2..19) ---
    virtual void OnInitialize();                       // slot 2
    virtual void OnPostInitialize();                   // slot 3
    virtual void OnActivate();                         // slot 4
    virtual void OnForeground();                       // slot 5  <- we override this
    virtual void OnBackground();                       // slot 6
    virtual void OnInactivate();                       // slot 7
    virtual void OnFinalize();                         // slot 8
    virtual void OnSuspend(bool& reboot);              // slot 9
    virtual void OnResume(const std::string& factor);  // slot 10
    virtual void OnEarlySuspend(bool& reboot);         // slot 11
    virtual void OnLateResume(const std::string& factor); // slot 12
    virtual void OnPreShutdown(bool& reboot);          // slot 13
    virtual void OnPreResetSetting();                  // slot 14
    virtual void OnResetSetting();                     // slot 15
    virtual void OnPostResetSetting();                 // slot 16
    virtual void StopBootAnimation();                  // slot 17
    virtual void StartResumeAnimation();               // slot 18
    virtual void StopResumeAnimation();                // slot 19

private:
    // Reserve the device object's real footprint so a subclass (CinderApp) on the stack/heap
    // is large enough for the DEVICE ApplicationBase ctor to write into. vptr is at +0, this
    // array starts at +4 (covering the LifeCycleManager* the ctor stores at this+4). See the
    // sizing note at the top of this file. The device ctor owns these bytes.
    alignas(8) unsigned char _easel_base_storage[0x3c];   // total sizeof = 4 + 0x3c = 0x40
};
static_assert(sizeof(ApplicationBase) >= kApplicationBaseRealSize,
              "ApplicationBase reserved storage smaller than the device object — would overflow");

// CuiAppModule — the non-Qt (framebuffer) module. Ctor signature CONFIRMED by demangle
// (libeaselcui _ZN5easel12CuiAppModuleC1E...): five void() callbacks, then a bool() pump,
// then one more void(). It derives easel::AppModuleBase<CuiAppModule> -> ModuleBaseInterface
// (single inheritance, ModuleBaseInterface is the primary base at offset 0 — so the
// unique_ptr<ModuleBaseInterface> upcast is a no-op pointer; we model it directly here).
// The 5 void cbs map to lifecycle steps (exact mapping TBC on device); `pump` is the render
// tick (return true to keep pumping).
class CuiAppModule : public ModuleBaseInterface {
public:
    CuiAppModule(ApplicationBase& app, int argc, char** argv,
                 std::function<void()> onInitialize,
                 std::function<void()> onPostInitialize,
                 std::function<void()> onActivate,
                 std::function<void()> onForeground,
                 std::function<void()> onFinalize,
                 std::function<bool()> onPumpTrigger,
                 std::function<void()> cb7);
    ~CuiAppModule() override;

private:
    // Reserve the device object's real footprint. `new easel::CuiAppModule(...)` must
    // operator-new enough room for the DEVICE ctor (libeaselcui), which writes the 7 std::function
    // callbacks, a std::mutex, a std::condition_variable and flags through this[0xfa]. Without
    // this we'd allocate only sizeof(vptr)=4 and corrupt the heap. vptr at +0, array at +4.
    alignas(8) unsigned char _device_object_storage[kCuiAppModuleRealSize + 0x40];
};
static_assert(sizeof(CuiAppModule) >= kCuiAppModuleRealSize,
              "CuiAppModule reserved storage smaller than the device object — would overflow the heap");

} // namespace easel
