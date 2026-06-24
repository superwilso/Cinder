# cinder-home — Cinder as a valid easel "Home" app (true Option-B Qt-app removal)

Make `appmgrservice` launch **Cinder** as the foreground `type:Home` app instead of the
stock Qt `HgrmMediaPlayerApp`, completing the app-manager **Foreground handshake** so the
device does **not** reboot. This is the clean way to remove the Qt app (frees its RAM, no
SIGSTOP), as opposed to the runtime SIGSTOP freeze in the safe `cinder_install.upg`.

Full protocol RE: [`../analysis/F_appmgr_home/RE_findings.md`](../analysis/F_appmgr_home/RE_findings.md).

## Why this shape
- `appmgrservice` `fork/exec`s the app, then **blocks on `WaitLifeCycleChanged(state=1
  Foreground, timeout)`**; on death/timeout → `Abort..` → `android_reboot`. (That's the loop
  the SIGKILL build hit.) A replacement must therefore *speak the protocol* and reach
  Foreground in time.
- The whole handshake is wrapped by **`easel::ApplicationBase::run(argc, argv, name, module)`**
  — it creates the `AppManagerModule` (connects to `appmgrservice`, registers the app, ACKs
  lifecycle) and runs the pump. We don't implement binder at all.
- The UI is a **`easel::CuiAppModule`** (non-Qt; `libeaselcui` needs only `libeaselcore`),
  constructed with **`std::function` callbacks** — so we pass lambdas, not a subclassed vtable.
  The render tick lives in the pump callback; we start/stop painting on Foreground/Background.

## Files
- `src/easel_abi.hpp` — hand-written declarations of `ApplicationBase` / `CuiAppModule` /
  `ModuleBaseInterface` reconstructed from RE (no SDK headers exist).
- `src/main.cpp` — the app: `CinderApp` + `CuiAppModule(callbacks)` + `run()`.
- `build.sh` — clang/libc++ armhf cross-build linking the device easel libs.
- `src/render.c` — *to add*: the framebuffer painter (reuse `player/cinder-device`'s fb
  open/ioctl/blit) exposed as the `cinder_render_init/tick/shutdown` C entry points, or an
  FFI surface into the Rust `cinder-ui`.

## Status: ABI LINK-VERIFIED against the real device libs (libc++). Only runtime libs remain.
Compiled `src/main.cpp` with **real clang-18 `-stdlib=libc++` `-fno-rtti`** (armhf) and confirmed
**every emitted undefined reference matches a device export exactly** — all 22 `easel::` refs
resolve against `libeaselcore.so`/`libeaselcui.so`, with real `std::__1` mangling (e.g.
`ApplicationBase::run(int,char**,char const*,std::__1::unique_ptr<…>)`). The companion
`cinder-audio` shim's 11 `PlayerService`/`PlayController` refs likewise all match
`libPlayerServiceClient.so`. So linking against the device libs is proven; the C++ ABI is
correct. (`-fno-rtti` is required — ApplicationBase's typeinfo is a non-exported local symbol;
see build.sh.)

The ABI surface was reconstructed from the **real** `libeaselcore.so`/`libeaselcui.so`
(symbol demangle + vtable relocation dump) and `src/easel_abi.hpp` reproduces it:

- ~~ApplicationBase's 2 pure virtuals.~~ **RESOLVED + verified byte-for-byte (2026-06-24).**
  Extracted `_ZTVN5easel15ApplicationBaseE` from `libeaselcore.so` (`.rel.dyn` `R_ARM_ABS32`
  entries): **22-word vtable** = offset-to-top, typeinfo, then **20 function slots**. Slots 0,1
  are both `__cxa_pure_virtual` → `ApplicationBase` has a **pure virtual destructor**
  (`virtual ~ApplicationBase() = 0`), satisfied by any concrete `~CinderApp()`. Compiling
  `main.cpp` with the cross compiler and dumping (`-fdump-lang-class`) shows `CinderApp`'s
  vtable is **22 entries** with `~CinderApp` in slots 0,1 and `OnForeground`/`OnBackground`
  overriding slots 5/6 — **identical length & order to the device** (the mis-sized/reordered
  vtable → wrong-slot-dispatch reboot class is eliminated).
- `easel::ApplicationBase::run(int,char**,char const*,unique_ptr<ModuleBaseInterface>)` and
  `SetPumpTriggerHandler(function<void()>)` mangle **identically** to the device exports ✓.
- `CuiAppModule` ctor confirmed `(ApplicationBase&, int, char**, function<void()>×5,
  function<bool()>, function<void()>)` — matches the device `_ZN5easel12CuiAppModuleC1E...` ✓.

**Remaining prerequisites (all toolchain/runtime, no more RE):**
1. **libc++ headers** to compile with `clang -stdlib=libc++` (so std types mangle `std::__1::`,
   not libstdc++'s `std::`). Not installed here (`libc++-18-dev`, needs apt/sudo, or fetch).
   g++/libstdc++ compiles the *structure* fine (proven above) but won't link the device libs.
2. **Device `libc++.so.1` + `libcxxrt.so.1`** to link and to match the runtime ABI. **NOT in the
   extracted rootfs** (`artifacts/rootfs_mnt` is missing libc++/libc.so.6 — the mount looks
   partial). Pull them off the device (`adb pull`/MSC) or from a fuller firmware extract.
3. **On-device validation** that host clang's libc++ `function`/`unique_ptr`/`string` *layout*
   matches the device's libc++ version (names match; layout must be confirmed by running it).

## ApplicationBase vtable — VERIFIED order (function slots 0–19, all `void` unless noted)
`~ApplicationBase (slots 0,1, pure)`, then: `OnInitialize, OnPostInitialize, OnActivate,
OnForeground, OnBackground, OnInactivate, OnFinalize, OnSuspend(bool&), OnResume(string const&),
OnEarlySuspend(bool&), OnLateResume(string const&), OnPreShutdown(bool&), OnPreResetSetting,
OnResetSetting, OnPostResetSetting, StopBootAnimation, StartResumeAnimation, StopResumeAnimation`.
(`ReadyToSuspend`/`ReadyToShutdown`/`Exit`/`GetAppParam`/`run` are exported but **non-virtual** —
not in the vtable. Defaults exist for all the virtuals; override only what you need.)
- `run()` internals: builds `AppManagerModule(argc,argv,name,handler=this->LifeCycleManager)`,
  sets power/reset handlers bound to `this`, registers `[AppManagerModule, userModule]`, runs
  `LifeCycleManager::Main`.

## On-device test plan (do it SAFELY)
Do **not** repoint the `.appcfg` until cinder-home reaches Foreground reliably — a failure =
reboot. Bring-up order:
1. Build cinder-home (after the two prerequisites). Stage it next to `cinder-device`.
2. Keep the **bad-boot counter** active (the safe wrapper) as the net: install cinder-home as
   an *alternative* launched behind the counter, so 3 failed boots auto-revert to stock in ~2 min.
3. First milestone: cinder-home launches, **reaches Foreground without rebooting** (verify via
   `appmgr` logs / the device staying up >60 s and the counter resetting). Painting can come after.
4. Then wire `render.c` and confirm Cinder paints as the real foreground app.
5. Only once stable: bake into CFW by setting `HgrmMediaPlayerApp.appcfg` `command:` →
   `cinder-home` (or replacing the binary) and repacking the rootfs (Phase-7 round-trip).
   Keep the wbrt backup + a revertable appcfg.

Until all that is proven, the **SIGSTOP safe build remains the way to run Cinder on screen.**
