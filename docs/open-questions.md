# NW-A55 Open Questions

Last updated: v1.5 — 2026-07-25 (re-audited: OQ2/OQ3/OQ6/OQ7 closed by on-device work;
OQ5 now moot). Live forward plan: [`../cinder-home/ROADMAP.md`](../cinder-home/ROADMAP.md).

---

- [x] **OQ1 — What is the SoC?**
  - **CLOSED (v1.4).** MediaTek MT8590, confirmed by `unknown321/wbrt` and Wampy
    `MAKING_OF.md`. The v1.3 retraction was itself an error. Phase 3 scripts updated to
    verify MT8590-specific markers rather than doing blind identification.
  - Primary sources: `github.com/unknown321/wbrt` README; Wampy `MAKING_OF.md`;
    USB VID `0x0E8D` (MediaTek preloader).

- [x] **OQ2 — USB-DAC + LDAC simultaneous: which layer enforces the block?**
  - **CLOSED (2026-06-22, confirmed on device).** The block is **Candidate 1: app policy** —
    `HgrmMediaPlayerApp` shows `disconnectMsgOverlay` and actively calls
    `IBtTransmitterService::RequestDisconnection()` on USB-DAC entry. Candidate 2 (SoundServiceFw
    routing mutex) and Candidate 3 (`llusbdac.ko`, which isn't in stock) are both **ruled out**.
    The on-device probe caught a capture stream (card4) and a playback stream (card0) **running
    concurrently with no mutex**, and RE showed **LDAC transmit is non-ALSA** (decoder →
    `BtTransmitterService` IPC → MTK BT chip), so the feature is a routing change, not a
    hardware-lock fight. Full detail: `../analysis/E_usbdac_ldac/RE_findings.md`, CLAUDE.md Part H.
  - **Still device-gated:** the *enforcement question is answered*; what remains is live end-to-end
    validation of the `ldac-bridge` daemon (capture card4 → LDAC socket) on the unit — see ROADMAP
    P2 and `../ldac-bridge/TEST.md`. Cinder's replacement UI already omits the overlay + the
    disconnect call.

- [x] **OQ3 — clang/LLVM vs GCC C++ ABI compatibility**
  - **CLOSED.** The device C++ ABI is **clang + libc++** (`HgrmMediaPlayerApp` and the Sony libs
    link `libc++.so.1` + `libcxxrt.so.1`; CLAUDE.md Part H4). The project's toolchain boundary is
    settled accordingly: the C++ device components (`cinder-home`, `cinder-audio`, `ldac-bridge`)
    build with **`clang -stdlib=libc++`** against the device-matching libs; `g++`/libstdc++ will
    **not** interop (std::__1::function/unique_ptr). The Rust UI stays musl/static and talks to the
    C++ layer over a C-FFI. Proven end-to-end: cinder-home constructs Sony objects cleanly under
    qemu against the device's own libs, and runs on the unit. See `../analysis/RE_playerservice_sound.md`
    (object-sizing method) and `../cinder-audio/src/*_abi.hpp`.

- [ ] **OQ4 — MT8590 boot ROM: SLA/DAA bypass applicability**
  - MediaTek BROM bypass tools exist (`mtkclient`) but their applicability to this
    specific NW-A55 unit (device-side fusing, SLA key state) is unknown.
  - Low priority unless eMMC-level access is needed (not required for replacement player).
  - Resolution: on-device; only if Phase 8 reveals a need for preloader-level access.

- [~] **OQ5 — Android base version** *(open but now MOOT for the build)*
  - Still not pinned to an exact Android release, but this **no longer blocks anything**: the
    device is up and the replacement player runs against its actual userspace. The confirmed,
    build-relevant facts are **Linux 3.10 (32-bit ARM) + glibc 2.23 + Qt 5.3 + Android-derived
    init/property infrastructure**, and those (not the marketing Android version) are what drive
    toolchain + 2038 decisions. Keep the "Android-derived init" characterization; don't assert a
    specific Android release without `getprop ro.build.version.release`. Low priority.

- [x] **OQ6 — Sony clang version and C++ standard library ABI**
  - **CLOSED (folds into OQ3).** Sony ships **libc++ (`libc++.so.1` + `libcxxrt.so.1`)**, not
    libstdc++. The safe FFI pattern is settled: build the C++ shims with `clang -stdlib=libc++`,
    call Sony methods **by mangled symbol** (non-virtual) or by **manually-reconstructed vtable
    index** (virtual, e.g. `BtTransmitterServiceClient`), and **reserve real object sizes** from the
    Ghidra decompiles before `new`-ing (the 2026-06-25 sizing bug + the `EffectCtrlDmp` 0xA8 fix).
    The C-FFI boundary to the Rust UI keeps the ABI risk contained.

- [x] **OQ7 — Scrobbler IPC mechanism**
  - **CLOSED — re-implemented, not IPC-hooked.** Cinder drives playback itself (Option B), so it
    knows track-state first-hand and **writes `/contents/.scrobbler.log` (Audioscrobbler/1.1)
    directly** as you listen — no dependency on the stock event bus or the `unknown321/scrobbler`
    daemon. Native, battery-efficient (goal #5). Implementation: `player/cinder-ffi/src/scrobble.rs`.

- [ ] **OQ8 — DMP-Z1 SoC identity**
  - `wbrt` lists DMP-Z1 as MT8590-based. Wampy `VOLUME_TABLES.md` states: "DMP-Z1 does
    not have these at all (different SOC)" and notes it uses an "Aulos card" instead of
    CXD3778GF.
  - This may mean DMP-Z1 shares the MT8590 application processor but has a different
    audio subsystem, or it may use a different SoC entirely that shares the MediaTek
    preloader interface.
  - **Low priority** for this project (DMP-Z1 is not the target), but worth noting for
    cross-model assumptions. Do not generalize NW-A55 findings to DMP-Z1 without
    verification.
