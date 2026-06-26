# cinder-home — Cinder as a valid easel "Home" app (true Option-B Qt-app removal)

> **Status & flash/verify guide: [`STATUS.md`](STATUS.md)** — start there. As of 2026-06-25 the
> OnInitialize boot crash is fixed (object-sizing overflow), cinder-home constructs cleanly
> (proven under qemu), and the UI is data-driven & daily-usable (real library browse + scroll,
> volume HUD, EQ, Bluetooth, scrobbler, full input pump). Build = `bash build.sh` (runs the
> GLIBC-2.23 gate + the qemu construction preflight); flash artifacts in `dist/`.

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

## Status: BUILDS — a real, device-loadable ARM binary (glibc-2.23-clean). VERIFIED 2026-06-24.
`build.sh` now produces a **2.5 MB ARM PIE** (`interp /lib/ld-linux-armhf.so.3`) that **needs
only `GLIBC_2.4`** — nothing newer than the device's glibc 2.23 — and **every undefined symbol
resolves against the device libraries** (cross-checked vs `libc/libc++/libcxxrt/libgcc_s` +
the Sony easel/PlayerService `.so`; the two "standard" imports `__stack_chk_guard`/
`__tls_get_addr` are imported by Sony's own working libs too, so they resolve at runtime).
All 22 `easel::` refs + 11 `PlayerService` refs link against the device `.so` with real
`std::__1` mangling. The device runtime libs were in the repo all along
(`analysis/ramdisk/lib/` — full glibc 2.23 + libc++/libcxxrt/libgcc_s, from the boot ramdisk);
NO device pull was needed. Remaining work is **on-device** only (bring-up + calibration), not
build. (`-fno-rtti` is required — ApplicationBase's typeinfo is a non-exported local symbol.)

### What it took to be glibc-2.23-clean (the hard part — see build.sh)
The host cross-toolchain is glibc 2.39; the device is glibc **2.23** (2016), and glibc is
backward- not forward-compatible, so a naive build emits `GLIBC_2.28..2.34` refs the device's
`ld-2.23` refuses. Three fixes: (1) a glibc-2.23 **sysroot** (Ubuntu-16.04 "xenial" armhf
`.debs`) for crt + libc, forced onto clang via `-B<crt>` + xenial libdirs (clang otherwise
silently uses the gcc-13/glibc-2.39 crt + `-lc`); (2) the bundled **SQLite** recompiled against
the 2.23 headers with **LFS off** (`-DSQLITE_DISABLE_LFS`) + **32-bit time** (`-U_TIME_BITS` —
glibc 2.23 has no `*_time64` symbols); (3) `src/glibc223_compat.c` shims `stat/fstat/lstat/
fstatat(+64)` → the `__xstat/__fxstat/…@GLIBC_2.4` the device actually exports (it doesn't
export plain `stat`, and SQLite takes `&stat`). NOTE re project goal #10 (Y2038): forcing
32-bit time here is REQUIRED for the 2.23 ABI and does NOT fix 2038 — genuine 2038-safety lives
in the musl components + i64 timestamps (see project-walkman-goals memory).

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

**Build prerequisites — ALL RESOLVED (offline, no device, no sudo):**
1. ~~libc++ headers~~ ✓ `apt-get download libc++-18-dev libc++abi-18-dev` → `dpkg-deb -x`.
2. ~~Device `libc++.so.1` + `libcxxrt.so.1`~~ ✓ already in `analysis/ramdisk/lib/` (the README
   earlier checked the WRONG tree — `rootfs_mnt`; the libs are in the boot-ramdisk extract).
3. ~~glibc-2.23 toolchain~~ ✓ xenial armhf `.debs` → `$DEVSYS` sysroot (build.sh PREREQUISITES).

**Remaining work — ON-DEVICE only (not build):**
1. Bring up behind the bad-boot counter; confirm it reaches appmgr **Foreground** without reboot.
2. `getevent` the physical-button → keycode map (the nav table) — the one true device unknown.
3. Ghidra the `PlayStatus` field offsets (position/duration; URI offset known) for live now-playing.
4. On-device validation that host clang's libc++ `function`/`unique_ptr`/`string` *layout* matches
   the device libc++ version (names match + the binary loads; layout confirmed by running it).

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
