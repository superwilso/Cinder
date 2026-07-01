# cinder-ffi — Rust Cinder UI as a glibc C-ABI staticlib

The retarget that makes Option-B a single process. The Cinder UI stays in **Rust** (no
rewrite — same `cinder-ui` render core, embedded-graphics + fontdue), but is built for
**`arm-unknown-linux-gnueabihf` (glibc)** as a `staticlib` exposing a **C ABI**, so it links
into the C++ easel shell [`cinder-home`](../../cinder-home/) alongside Sony's glibc/libc++
libraries. (The musl-static [`cinder-device`](../cinder-device/) — the SIGSTOP-overlay safe
build — is unchanged and still builds for `armv7-unknown-linux-musleabihf`.)

## Why
- **Performance:** none lost — Rust is native, no GC; a 2D 480×800 panel is memcpy/glyph
  bound, identical to C. (Default triple is ARMv6 baseline; add `RUSTFLAGS="-C
  target-cpu=cortex-a7 -C target-feature=+neon"` later for ARMv7/NEON.)
- **Compatibility:** musl-static Rust can't share a process with glibc/libc++. glibc Rust +
  C ABI links cleanly — proven (a C harness cross-links against `libcinder_ffi.a` with just
  `-lpthread -ldl -lm`).

## API — `include/cinder.h`
The C ABI the shell drives. State lives behind a `Mutex`; `panic="abort"` so nothing unwinds across
the boundary (a single panic aborts before any poison can cascade). Grouped:
- **Render/lifecycle:** `cinder_render_init` (open `/dev/graphics/fb0`; blit bounded against the
  mmap), `cinder_render_tick` (one dirty-flagged frame from the easel pump), `cinder_render_shutdown`,
  `cinder_set_theme_night`.
- **Input/nav:** `cinder_input(button)` → returns a `cinder_action_t` (0–15) for the shell to carry
  out (transport, EQ-changed, battery-care-changed, sound-changed, sound-bypass, …).
- **Now playing / library:** `cinder_db_open`, `cinder_set_now_playing[_uri]`, `cinder_clock_tick`,
  `cinder_set_battery`.
- **Effects/settings read-back:** `cinder_get_eq_bands`, `cinder_get/set_battery_care`,
  `cinder_get_sound_flags`, `cinder_get_sound_bypass` (the shell applies these via the cinder-audio shims).
- **Visualiser:** `cinder_set_visualizer[_type]`, `cinder_visualizer_count`, `cinder_set_pcm`
  (our FFT) / `cinder_set_spectrum` (Sony analyzer bands).
- **Scrobbler:** `cinder_scrobble_open` / `cinder_scrobble_tick`.

Full per-feature status (functional / partial / stationary): **`../../cinder-home/STATUS.md`**.

## Features (build channel)
`dev` (default off) forwards to `cinder-ui/dev` and flips the on-device Firmware marker to
`CINDER DEV`; the stable build omits it. `cinder-home/build.sh dev` passes `--features dev`.

## Build
```bash
cd player
cargo build -p cinder-ffi --release --target arm-unknown-linux-gnueabihf                 # stable
cargo build -p cinder-ffi --release --target arm-unknown-linux-gnueabihf --features dev   # dev
# -> target/arm-unknown-linux-gnueabihf/release/libcinder_ffi.a
```
Then `cinder-home/build.sh [stable|dev]` links it in.
