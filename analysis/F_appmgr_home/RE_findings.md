# appmgr / easel Home-app protocol — RE findings

**Question:** how to remove/replace the stock Qt UI (`HgrmMediaPlayerApp`) and run Cinder as
the foreground app **without the boot loop** that SIGKILL caused (2026-06-23).

**Answer in one line:** the foreground UI is a managed *"Home" app*; the app-manager
(`appmgrservice`) launches it, then **blocks waiting for it to report lifecycle state =
Foreground within a timeout**, and **reboots the device** (`android_reboot`) if it doesn't.
To replace it cleanly, Cinder must become a *valid* app that completes that handshake — which
the `easel` framework makes a ~one-call affair.

---

## 1. The cast
| Piece | Where | Role |
|---|---|---|
| `appmgrservice` | binder service in `hagoromo2` (`hagodaemon appmgrservice sub_sm`), code in `libappmgrservice.so` | launches apps, drives their lifecycle, reboots on failure |
| `HgrmMediaPlayerApp.appcfg` | `vendor/sony/bin/` (78 bytes) | `name`/`command:HgrmMediaPlayerApp`/`type:Home`/`hidden:false` — the **only** app registered |
| `pst::appmanager::AppManager` | client class, **also in `libappmgrservice.so`** | the app-side binder client (connect/register/ACK) |
| `easel::ApplicationBase` / `easel::QtApplication` | `libeaselcore.so` / `libeaselqt.so` | base class an app subclasses; `run()` does the whole bootstrap |
| `pst::core::Framework::GetServiceClient` | `libpstcore.so` | binder service-client lookup ("appmgrservice") |

`HgrmMediaPlayerApp` links `libeaselcore`+`libeaselqt`+`libpstcore` (and pulls `AppManager`
via `libappmgrservice`). It is **not** in `init.rc` — `appmgrservice` starts it.

## 2. The launch → foreground → reboot protocol (decompiled)
Server side, `libappmgrservice.so`:

- `AppManagerServiceImpl::StartApp(name)` → `LifeCycleManager::StartApp` →
  `LifeCycleController::InvokeAndSetForeground(appInfo, &pid)` (@0x24974):
  1. `ProcessController::Invoke` = **`fork()` + `execvp(command)`** (command from the `.appcfg`).
     Log: `%s has started. pid = %d`.
  2. **`AppRegistry::WaitLifeCycleChanged(app_id, state=1, timeout)`** — blocks until the app
     reports it reached **lifecycle state 1 (Foreground)**, or the timeout fires.
  3. On failure/timeout: `Failed to make %s foreground (%d). Abort..` →
     `ServiceManager::NotifyCrash` / `DoSaveCurrentLog(needReboot=1)` → **`android_reboot`**
     (in `libpstcore.so`). Also: `Application process is killed! appmgrservice will exit...`.

- `LifeCycleController::ChangeLifeCycle(appInfo, state)` (@0x24bf8): fires the transition to the
  app, then `WaitLifeCycleChanged(app_id, state, timeout)` again — every transition is an
  ACK-with-timeout. Returns error 4 on timeout (`Failed to change lifecycle %s to …`).
- Timeout machinery: `AppManager::Counter::StartTimeoutCheck/StopTimeoutCheck` (JobQueue +
  `condition_variable`); budget is per-controller and the app can declare it via
  `ApplicationBase::GetChangeLifeCycleTimeout()` / `GetPostInitializeTimeout()`.

Client side (the app), `AppManager::Initialize(name, ChangeLifeCycleHandler&)` (@0x20c9c):
  1. `Framework::GetServiceClient("appmgrservice")`.
  2. Registers an `AppManagerServiceListener` (filtered by the app name) on that client
     (RegisterListener via vtable+0x38; `ListenerDescUtil` builds the filter).
  3. Reports `InitializeComplete(name, pid)`.
  4. Thereafter the server fires `ChangeLifeCycle(name, state)` → listener →
     `AppManager::ChangeLifeCycle` → the app's `ChangeLifeCycleHandler` runs the transition →
     app calls `ChangeLifeCycleComplete(name, state, result)` → server's `WaitLifeCycleChanged`
     unblocks.

Inside `easel` (`libeaselcore.so`) the handler is `LifeCycleManager`, which walks the app's
modules through `ToInitialize → ToPostInitialize → ToActivate → OnForeground` (each step calls
the module vtable slots +8/+0xc/+0x10/+0x14; `OnForeground` sets the manager's state flag = 1).

### life_cycle_t states (inferred)
`1` = Foreground/Active (the post-launch target `WaitLifeCycleChanged` waits for). `2`/`3` are a
Background/Inactive/Suspend group (`(state & ~1) == 2` is special-cased). `0` = uninitialized.
Internal sub-steps Initialize/PostInitialize/Activate run inside easel before it signals state 1.

## 3. Why each displacement behaves as it does
- **SIGKILL the Qt app → boot loop.** The Home app dies → appmgr relaunches it / the foreground
  wait fails → `Abort..` → `android_reboot`. Repeats every boot. *(What happened 2026-06-23.)*
- **SIGSTOP the Qt app → safe.** The real app *already reached Foreground* (handshake done)
  before we froze it, so appmgr stays satisfied with a live, foreground process. Cost: the
  frozen app's RAM. *(Current safe build.)*
- **Repoint `.appcfg` to a bare `cinder-device` → boot loop.** appmgr would `execvp` it, then
  `WaitLifeCycleChanged(state=1)` would time out (cinder-device speaks no appmgr protocol) →
  `Abort..` → reboot. **Do not do this.**

## 4. The clean removal — make Cinder a *valid* Home app
The app skeleton is tiny because `easel` encapsulates the whole handshake. From
`HgrmMediaPlayerApp`'s dynamic imports, the bootstrap is:

```
easel::QtApplication::QtApplication(int&, char**)          // Qt variant (HgrmMediaPlayerApp)
easel::ApplicationBase::run(int, char**, char const* name,  // THE one-call bootstrap
                            unique_ptr<ModuleBaseInterface> module)
easel::ApplicationBase::{OnInitialize,OnForeground,OnSuspend,ReadyToShutdown,
                         GetChangeLifeCycleTimeout, StopBootAnimation, ...}  // overridable hooks
```

`run(argc, argv, name, module)` internally builds the `AppManagerModule` (which calls
`AppManager::Initialize(name, handler)`), registers the module(s), and runs the
`LifeCycleManager` pump — i.e. it performs the connect + Foreground handshake + timeout for us.
The app just overrides the hooks.

**Path A (recommended): a thin C++ "cinder-home" easel app.**
- Toolchain: `arm-linux-gnueabihf` + libc++/libcxxrt (the same boundary as `ldac-bridge`),
  linking `libeaselcore.so` + `libpstcore.so` (+ `libappmgrservice.so`). *Not* the Cinder musl
  toolchain.
- Subclass `easel::ApplicationBase`; override `OnForeground()` to start the Cinder framebuffer
  render loop (call into the existing Rust render core via a C FFI entry, or drive
  `/dev/graphics/fb0` from C++ and keep the Rust UI as a library). Default `GetChangeLifeCycleTimeout`
  is fine to start.
- `main()` ≈ `CinderApp app; app.run(argc, argv, "HgrmMediaPlayerApp", <minimal module>); `.
- Result: `appmgrservice` launches Cinder as the Home app, Cinder completes the Foreground
  handshake → **no reboot, Qt app never runs, its RAM is freed.** This is the true Option-B
  removal.
- Implementation detail to nail: our subclass's **vtable layout must match
  `easel::ApplicationBase`** (`_ZTVN5easel15ApplicationBaseE`) — derive the virtual order from
  `libeaselcore` and hand-write a matching class decl (no SDK headers exist). Also confirm a
  *non-Qt* `ApplicationBase` is allowed to own the framebuffer (HgrmMediaPlayerApp uses the Qt
  variant; the panel is plain `/dev/graphics/fb0`, so a non-Qt app should be fine, but verify).

**Path B (fallback): reimplement the `appmgrservice` binder client from scratch** (Rust/C) —
the `Initialize` / `RegisterListener` / `ChangeLifeCycleComplete` wire format over the pst
binder transport. Avoids the C++ ABI but is far more RE. Only if Path A's C++ linkage proves
unworkable.

## 5. CFW integration (user is open to custom firmware)
With CFW the launch target is changed at the source instead of hooked at runtime:
- Edit `HgrmMediaPlayerApp.appcfg` `command:` → `cinder-home` (or replace the
  `HgrmMediaPlayerApp` binary with our easel app), drop `cinder-home` into
  `vendor/sony/bin/`, repack the rootfs (sector 6) into the `.UPG` (Phase 7 proved the
  round-trip), flash. appmgr then launches Cinder as the `type: Home` app from boot — no runtime
  wrapper, no Qt process at all.
- **CFW does not remove the handshake requirement** — Cinder still must reach Foreground (Path
  A), or appmgr reboots. CFW just changes *what* is launched.
- Keep the safety net regardless: a revertable `.appcfg`/binary (reflash to undo) and the wbrt
  backup. A bad-boot-style guard is still wise during bring-up.

## 6. Recommended next steps
1. **Now:** ship the SIGSTOP safe build to get Cinder on screen (already staged); it cannot loop.
2. **Build `cinder-home` (Path A):** decompile `HgrmMediaPlayerApp`'s `main`/entry for the exact
   `run()` call + module construction, hand-write the `easel::ApplicationBase` vtable-matching
   class, override `OnForeground` to start rendering, link the easel libs, test as the Home app
   **behind the bad-boot counter**. When it reaches Foreground reliably without rebooting, that's
   the true Qt-app removal — then optionally bake it into CFW via the `.appcfg`.

## 7. Open items
- Exact numeric `life_cycle_t` map (confirm 1=Foreground and the 2/3 group) — decompile
  `AppRegistry::WaitLifeCycleChanged` + the enum.
- `ApplicationBase::run` module argument: minimal `ModuleBaseInterface` Cinder must pass.
- Whether a non-Qt `ApplicationBase` can own `/dev/graphics/fb0` cleanly (vs. `QtApplication`).
- The `easel::ApplicationBase` vtable order (for the C++ subclass) — from `_ZTVN5easel15ApplicationBaseE`.

## 8. Implementation recipe (confirmed) — `cinder-home`
Scaffolded at `../../cinder-home/` (blueprint). The clean app, no Qt, no vtable subclassing
of the module:
- `run()` (libeaselcore @0x4574) builds `AppManagerModule(argc,argv,name, handler=this's
  LifeCycleManager)`, sets power/reset handlers bound to `this`, registers
  `[AppManagerModule, userModule]`, then `LifeCycleManager::Main(registry)` runs the pump.
- The UI module is **`easel::CuiAppModule`** (`libeaselcui`, needs only `libeaselcore` — NO Qt).
  Demangled ctor: `CuiAppModule(ApplicationBase&, int argc, char** argv, function<void()>×5,
  function<bool()> pump, function<void()>)`. Pass lambdas; the `bool` one is the render tick
  (`OnPumpTrigger`). So: `app.run(argc,argv,"HgrmMediaPlayerApp", make_unique<CuiAppModule>(...))`.
- `ApplicationBase` vtable (`_ZTVN5easel15ApplicationBaseE @0x19a18`): slots [0],[1] =
  `__cxa_pure_virtual` (a concrete app MUST override), [2..]=`OnInitialize, OnPostInitialize,
  OnActivate, OnForeground, OnBackground, OnInactivate, OnFinalize, OnSuspend, OnResume,
  OnEarlySuspend, OnLateResume, OnPreShutdown, OnPreResetSetting, OnResetSetting,
  OnPostResetSetting, ReadyToSuspend, ReadyToEarlySuspend, ReadyToShutdown, StopBootAnimation,
  StartResumeAnimation, StopResumeAnimation` (all have defaults).

### Two hard prerequisites before `cinder-home` builds/runs
1. **libc++ ABI toolchain.** easel symbols use `std::__1::function`/`unique_ptr` (libc++) →
   build with **clang `-stdlib=libc++`** against a libc++ matching the device's `libc++.so.1`
   (clang-18 present on host; runtime ABI must be device-validated). NOT g++/libstdc++.
2. **The 2 pure virtuals' identity** (vtable slots 0,1) — unresolved. Resolve by decompiling
   `CuiAppModule`'s ctor / a concrete consumer, or `QtApplication`'s easel *secondary* vtable
   (Qt multiple-inheritance made the linear dump unreliable — needs a construction-vtable-aware
   pass). Until then `CinderApp` is abstract and won't instantiate.

## Artifacts
`easelcore.c`, `appmgr.c`, `easel_run.c`, `appbase_vtable.txt` (Ghidra, this dir); reusable
`analysis/E_usbdac_ldac/ghidra/DecompileByName.java` + `DumpVtableSym.java`. Scaffold:
`cinder-home/` (README + src/easel_abi.hpp + src/main.cpp + build.sh).
