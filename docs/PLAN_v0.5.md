# Cinder v0.5.0: a new app on today's shell

**State (2026-11-25): P0 and P1 done.**
- P0: `cinder-core`, `cinder-app5` with the stall test, `CINDER_APP=5` builds.
- P1: `cinder-ui/src/look.rs` reads a theme folder's `theme.conf` (tokens and component variants)
  and draws it through `Style::Data`. `cinder-ui/themes/` ships Cinder plus three examples, and a
  test proves the Cinder theme draws exactly what the code does. `cinder-host --v5` checks and
  previews every theme folder.

- P2 (offline part): `cinder-ui/src/np5.rs` (Now Playing at Big and Huge from the boards, the chosen
  toolbar), `sheet5.rs` (this song's actions, Places), the app drawing them with taps, buttons and
  swipes, and D1 behind `/contents/cinder_fb_queue` (`cinder-core/src/ovl.rs`). Next: the first
  player run (`tools/cinder-install.sh --v5`), then P3. v0.4 gets fixes only until v0.5 replaces it.

## What v0.5 is
v0.5.0 is a separate Cinder app, built alongside v0.4 in this repo. It replaces v0.4 when it is
complete. Its goals:

- **Efficiency and modularity** built in from the start.
- **Themes v2:** people can design their own UI, not only its colours.
- **No stalls.** Drawing has its own thread, so a tap shows at once and anything slow shows that it
  is loading.
- **The 2026-11-24 redesign.** It is Big-first, Now Playing is home, and it has fewer options.

It keeps the C++ shell (`cinder-home/src/main.cpp`) and the C interface (`cinder.h`). Only the Rust
app on top is new.

## Why v0.4 stalls
- **One lock guards everything.** A single `Mutex<Option<Render>>` (`player/cinder-ffi/src/lib.rs`,
  `cell()`) holds UI state, the library, caches, playback, network and settings. About 220 of the 239
  `extern "C"` functions take it.
- **The frame holds that lock throughout.** `cinder_render_tick` keeps it for:
  - the whole raster (about 16 ms);
  - first-use loads: a font up to 2 MB, a `.wsz` file, tape frames up to 24 MB;
  - a synchronous album cover read;
  - the blocking hand-off to the present thread.
- **Other slow work under the lock:**
  - `save_settings`, on every tap;
  - `art_cache_plan`, 457 ms at boot;
  - the end of a library reload, about 144 ms;
  - stats flushes;
  - lyrics loads on a track change.
- **Audio can wait on drawing.** The AirPlay reader feeds the jack and takes the same lock, so audio
  can wait behind a frame.

## The approach: split by data, not a new UI framework
v0.5 reuses the drawing code that exists: `cinder-ui`'s `Canvas`, fonts and glyph cache, icons, the
kit, `present.rs` and `widgets`. It adds no widget tree, no layout engine and no scripting. What
changes is who owns which state and which thread does what.

**Reuse first.** Code that is reused as it is:
- `cinder-db`;
- every service;
- the input classifier;
- the settings file keys.

Every screen is analysed and probably redesigned. Reuse means code, not looks: a v0.4 screen module
is the starting point for its v0.5 screen.

**Fewer options.** Every row and setting is either cut, given a default instead of a switch, merged
into another, or moved to Advanced. Only what survives is built.

**Gestures that stay:**
- tap the clock to reach Now Playing;
- swipe in from the left edge to go back;
- swipe up for the Shelf;
- pull down for the quick panel.

### Crates
| Crate | What it holds |
|---|---|
| `cinder-core` (new) | The services, moved out of `cinder-ffi` unchanged so both apps can link them, plus `Backend`: the thread that owns the library, queue, likes, stats and settings. v0.4's `cinder-ffi` then depends on it. |
| `cinder-app5` (new, staticlib) | The v0.5 app: its navigator, its threads, and the same `cinder.h` exports. |
| `cinder-ui` (existing) | Drawing primitives and screen modules for both apps, plus the Themes v2 loader in `look/`. |

### Threads
| Thread | Owns | Talks by |
|---|---|---|
| Shell threads (unchanged) | the device | calling `cinder.h`; calls that change something only enqueue a command |
| Backend | services, library, queue, settings | commands in; a read-only, versioned snapshot out (a pointer swap) |
| IO workers | covers, lyrics, network, file loads | results back to the backend as commands |
| UI | screen state: screen, scroll, sheet, pressed, size | reads the latest snapshot and sends commands; never takes a lock the backend can hold |
| Present | the panel | `present.rs`, as today |

How these threads behave:
- **Taps answer in the same frame.** The pressed look, navigation, scrolling and sheets all happen
  on the UI thread.
- **Side effects show as pending.** A like, a queue edit or a setting shows as pending until the
  backend confirms it.
- **Slow resources show a loading look.** Each one is Loading, Ready or Failed, and the theme draws
  a placeholder while it loads.

**Budgets:**

| Measure | Limit |
|---|---|
| UI thread blocked by the backend | never more than 1 ms |
| Raster at Big or Huge | no more than 16 ms |
| Snapshot publish | no more than 1 ms |
| Extra memory | no more than 8 MB |

### Themes v2
A theme is a folder, `/contents/cinder_themes/<id>/`. Its `theme.conf` uses the palette file's
`key = value` syntax. It has four layers:

1. **Tokens.**
   - Colours: today's palette keys, so palette files keep working.
   - Type roles and weights, and one font.
   - Shape: radius and strokes.
   - Density: Big (the default), Huge, and Compact under Advanced.
2. **Components.** One variant for each kit part: row, chosen row, chip, switch, play button,
   header, return bar, toolbar, sheet, and the loading placeholder.
3. **Layouts.** A slot file for each screen.
   - It is resolved once, when the theme loads, into fixed rectangles, so it costs nothing per frame.
   - It must pass the layout contract  or it is refused with a reason:
     every required control is present; targets are at least 44 px, or 56 px in Big; the seek bar's
     grab band is clear; text fits at 140 %; it is readable on every palette; it stays within budget.
4. **Assets.** A background, an icon sheet, a cover frame and transport glyphs, 2 MB at most,
   scaled once on load.

The built-in Cinder theme ships as files, and a test proves it draws exactly as the code defaults
do.

Themes v2 reuses:
- the palette parser and its contrast checks;
- the `contracts/` corpus;
- Flint's port of the parser;
- the cinder-themes checker.

This replaces the 2026-09-14 "skins only" decision in [`PLAN_skins.md`](PLAN_skins.md) and
[`PLAN_now_playing_layouts.md`](PLAN_now_playing_layouts.md). Two worries from those documents and
how v0.5 answers them:
- **Speed:** layouts are resolved once at load, not every frame.
- **Taps landing in the wrong place:** drawing and taps read the same rectangles.

## GPU and display hardware (researched 2026-11-25)
**Verdict: no GPU rendering. Use the display hardware instead.**

Why not the Mali-450 GPU:
- **It was tried and lost.** A GLES 2 present path was built and measured in 2026-07: 45.6 ms a
  frame against 9.6 ms in software (4.7× slower). It was deleted in 2026-10.
- **Cinder can't reach it safely.** EGL needs `/dev/ion`, `/dev/sw_sync`, `/dev/mtk_disp` and
  `/dev/mtkfb_vsync`, which are root-only. Opening them up once hung the player at boot.
- **It costs power and a rewrite.** The GPU clock (MMPLL) is off while unused, and a GL renderer
  is the new UI framework this plan rules out.
- **A GPU visualiser doesn't avoid this.** It still needs the same GL present path. The
  visualiser's own CPU cost has never been measured, so measure it before moving it anywhere.

Where the frame time really goes:
- The panel runs in video mode. `FBIOPUT_VSCREENINFO` turns the overlay layers off, queues a
  full-screen compose, then **sleeps until the next frame** (up to ~16 ms). That sleep is most of
  the 9–10 ms "flip".
- Raster is always the full screen; only the copy to the framebuffer skips unchanged rows.

What v0.5 does instead, in `cinder-core/src/display.rs`, smallest first:

| Step | Change | Expected gain |
|---|---|---|
| D1 | Flip with `MTKFB_QUEUE_OVERLAY_CONFIG` (returns a fence at once) instead of `FBIOPUT_VSCREENINFO` | the present thread stops sleeping; scroll frames limited by raster (7–8 ms), not present (~14.5 ms) |
| D2 | Dirty rectangles in the raster: redraw only what changed | most frames are a clock, a progress bar or a pressed row |
| D3 | The visualiser in its own small overlay layer (alpha-blended over Now Playing) | it updates without redrawing or copying the screen |
| D4 | Experiment: scroll a pre-drawn strip by moving the layer's source offset | no copy while flinging; limited to the ~1.4 MB spare in the framebuffer |

Rules for this work:
- Never mix the overlay calls with the old pan call: every pan switches layers 1–3 off.
- Never read from `/dev/graphics/fb0`: it is write-combined.
- Each step is a dev-build switch with its own player run, measured with `devctl.sh stalls`.
- D1 lands with P2 (the first run on a player). D2 lands with P2's Now Playing. D3 and D4 come
  after P3.

## Phases
Each phase ends with `tools/ci_checks.sh` green and v0.4's behaviour unchanged.

| Phase | Delivers |
|---|---|
| P0 Foundations | `cinder-core` with the services that have no UI ties moved out unchanged. `cinder-app5` exporting all of `cinder.h`, with the command/snapshot/UI-thread core and a stall test. A build switch (`CINDER_APP=5`, dev channel only). The shell harness run against it. |
| P1 Look v2 core | Tokens (shape and density added), component variants, the Cinder theme as files, the checker, `cinder-host --v5` previews. |
| P2 Now Playing | Big and Huge sizes. A labelled toolbar you choose (default: Library, Up Next, Bluetooth, Places). Tap the title for this song's actions. The Places sheet and the return bar. First run on a player. |
| P3 Library, Up Next, Song info | A header with the view picker, one Shuffle / Sort / Filter bar, grid and list, search, Up Next. A Song info page for any song, opened from a row's actions or from the title. |
| P4 Settings | Six groups and the Size sheet, with every settings page reviewed. |
| P5 Every other screen | Each screen analysed, redesigned and built, ticked against [`STATUS.md`](../cinder-home/STATUS.md) so no feature is lost. |
| P6 Open themes | Layout and asset files, the cinder-themes checker for v2, Flint's theme editor. |
| P7 Switch-over | Settings carried over, the installer, v0.5.0 release candidates, v0.4 retired. |

## How each screen is done
1. Render the v0.4 screen at Big size (`cinder-host --big`).
2. Analyse it: what to cut, merge or move, and what matters most on it.
3. Draw the v0.5 board.
4. Build it in `cinder-app5`, starting from the v0.4 module.
5. Compare `cinder-host --v5` previews with the board, then with a screenshot from the player.

## Checks
- **Moving code:**
  - test counts are the same before and after;
  - v0.4's golden previews are unchanged;
  - `nm` shows every `cinder_*` symbol in both libraries.
- **Stall test:** while the backend sleeps 500 ms, the UI draws 30 frames, none anywhere near the
  500 ms a wait would cost, and no call from the shell blocks for more than 1 ms. The 16 ms raster
  budget is measured on the player.
- **On a player:** `tools/devctl.sh stalls` shows nothing over 50 ms during a library reload, a
  cover fling or opening Settings.
