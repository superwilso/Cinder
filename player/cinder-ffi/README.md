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
`cinder_render_init` (open `/dev/graphics/fb0`), `cinder_render_tick` (one frame, called from
the easel pump), `cinder_render_shutdown`, `cinder_set_theme_night`, `cinder_set_now_playing`.
State lives behind a Mutex; `panic="abort"` so nothing unwinds across the boundary. The
surface will grow (input events, screen selection, library/queue) as the IPC layer lands.

## Build
```bash
cd player
cargo build -p cinder-ffi --release --target arm-unknown-linux-gnueabihf
# -> target/arm-unknown-linux-gnueabihf/release/libcinder_ffi.a
```
Then `cinder-home/build.sh` links it in.
