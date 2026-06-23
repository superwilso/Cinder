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

## Status: BLUEPRINT — not yet buildable/runnable. Two hard prerequisites:
1. **libc++ ABI toolchain.** The easel symbols use `std::__1::function` / `unique_ptr`
   (libc++). Must build with **clang `-stdlib=libc++`** against a libc++ whose ABI matches
   the device's `libc++.so.1` (host clang-18's libc++ headers are a starting point; the
   runtime ABI must be validated on-device). g++/libstdc++ will *not* interoperate.
2. ~~ApplicationBase's 2 pure virtuals.~~ **RESOLVED (2026-06-23).** Imported the stripped
   `HgrmMediaPlayerApp`, found its concrete app-class `ApplicationBase` vtable (anchored on the 3
   inherited tail slots `StopBootAnimation`/`StartResumeAnimation`/`StopResumeAnimation`), and
   decompiled vtable slots 0,1: they are the **complete (D1) and deleting (D0) destructors** —
   i.e. `ApplicationBase` has a **pure virtual destructor** (`virtual ~ApplicationBase() = 0`),
   not two mystery abstract methods. A concrete subclass satisfies it just by **having a
   destructor** (`~CinderApp()`), which it does anyway. No extra methods to implement. The
   vtable order is confirmed: `~dtor(D1,D0), OnInitialize, OnPostInitialize, OnActivate,
   OnForeground, OnBackground, OnInactivate, OnFinalize, OnSuspend, OnResume, …`. So the ONLY
   remaining prerequisite is the libc++ toolchain (#1) + on-device validation.

## Confirmed (the RE that *is* done)
- `ApplicationBase` vtable order (slots 2–21): `OnInitialize, OnPostInitialize, OnActivate,
  OnForeground, OnBackground, OnInactivate, OnFinalize, OnSuspend, OnResume, OnEarlySuspend,
  OnLateResume, OnPreShutdown, OnPreResetSetting, OnResetSetting, OnPostResetSetting,
  ReadyToSuspend, ReadyToEarlySuspend, ReadyToShutdown, StopBootAnimation, StartResumeAnimation,
  StopResumeAnimation` (defaults exist — override as needed).
- `run()` internals: builds `AppManagerModule(argc,argv,name,handler=this->LifeCycleManager)`,
  sets power/reset handlers bound to `this`, registers `[AppManagerModule, userModule]`, runs
  `LifeCycleManager::Main`.
- `CuiAppModule` ctor (demangled): `(ApplicationBase&, int, char**, function<void()>×5,
  function<bool()>, function<void()>)`. The `bool` one is the pump/`OnPumpTrigger` tick.

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
