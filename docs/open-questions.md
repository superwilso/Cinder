# NW-A55 Open Questions

Last updated: v1.4

---

- [x] **OQ1 — What is the SoC?**
  - **CLOSED (v1.4).** MediaTek MT8590, confirmed by `unknown321/wbrt` and Wampy
    `MAKING_OF.md`. The v1.3 retraction was itself an error. Phase 3 scripts updated to
    verify MT8590-specific markers rather than doing blind identification.
  - Primary sources: `github.com/unknown321/wbrt` README; Wampy `MAKING_OF.md`;
    USB VID `0x0E8D` (MediaTek preloader).

- [ ] **OQ2 — USB-DAC + LDAC simultaneous: which layer enforces the block?**
  - Three candidates (baseline §5.10):
    1. **App policy** — player app calls BT disconnect on USB-DAC entry (most likely; Sony
       UI shows a confirmation dialog → app-layer logic)
    2. **libSoundServiceFw routing exclusivity** — source/sink enum rejects
       simultaneous audio paths
    3. **llusbdac.ko kernel module** — binds a fixed ALSA sink, no loopback
  - Resolution: on-device `strace` + `aplay -l` comparison across audio modes
    (CLAUDE.md Part E4–E5). Do not promise feature before this experiment.

- [ ] **OQ3 — clang/LLVM vs GCC C++ ABI compatibility**
  - Sony's toolchain version is unknown. If HgrmMediaPlayerApp was compiled with
    clang (LLVM ABI) and the replacement player links against system libstdc++ (GCC ABI),
    virtual function tables and exception handling may mismatch.
  - Resolution: Phase 4c (`analysis/4c_compiler_version.txt`) — check compiler version
    strings in HgrmMediaPlayerApp and libSoundServiceFw.

- [ ] **OQ4 — MT8590 boot ROM: SLA/DAA bypass applicability**
  - MediaTek BROM bypass tools exist (`mtkclient`) but their applicability to this
    specific NW-A55 unit (device-side fusing, SLA key state) is unknown.
  - Low priority unless eMMC-level access is needed (not required for replacement player).
  - Resolution: on-device; only if Phase 8 reveals a need for preloader-level access.

- [ ] **OQ5 — Android base version**
  - Audit 2 claimed Android 5.0 (Lollipop) with ART/Dalvik removed. The init
    infrastructure (init.rc syntax, property service, adb availability) is consistent
    with this, but the specific Android version has not been independently verified.
  - Defensible characterization: "Linux 3.10 + Qt 5 + Android-derived init/property
    infrastructure." Do not state Android 5.0 without on-device confirmation.
  - Resolution: `getprop ro.build.version.release` or `ro.build.id` on the device shell.

- [ ] **OQ6 — Sony clang version and C++ standard library ABI**
  - Related to OQ3. Specifically: does Sony ship a custom libstdc++ or libc++? Which
    C++ standard version was used? This determines which Rust FFI patterns are safe for
    the C shim boundary.
  - Resolution: Phase 4c output + strings in libstdc++.so on rootfs.

- [ ] **OQ7 — Scrobbler IPC mechanism**
  - The existing `unknown321/scrobbler` daemon receives track-state events somehow
    (D-Bus? Unix socket? shared memory?). The replacement player must emit the same
    events, or re-implement the log-writing itself.
  - Resolution: read `scrobbler/playerevents/` source; trace IPC on device.

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
