# Swappable UIs — palettes, then skins

*Written 2026-09-11. Steps 0 and 1 are done and host-tested; steps 2–4 are the plan.*

The goal: a standard way to design a new look for Cinder and swap it in and out, without forking the
app and without a second copy of the logic that decides what a tap does.

## Where it starts from

[`player/cinder-ui/src/nav.rs`](../player/cinder-ui/src/nav.rs) is the whole app brain — state,
input and screen dispatch — in one `App`: about 13,000 lines, 7,500 of them before the tests. Layout
is tangled through it:

| Measured 2026-09-11 | |
|---|---|
| Functions `nav.rs` calls in the screen modules | 164, of which 70+ answer layout questions (`hit_*`, `*_at`, `row_top_px`, `max_scroll_px`, …) |
| Geometry hard-coded in `nav.rs` itself | Now Playing's five transport buttons are literal circles, e.g. `hit(240, 692, 44)` |
| Coordinate-driven calls in `nav.rs`'s tests | 213 taps, swipes and scrolls against today's exact layout |
| Font sizes written as literals in the screen modules | 314 |
| References from `nav.rs` into `library.rs` alone | ~300 |

Two things are already the right shape. `App::render` builds a view struct for most screens and
hands it to that screen's `render`. And eight screens already answer a tap with a named result
rather than a coordinate (`confirm::Hit`, `BtHit`, `PairHit`, `ShelfHit`, `AlbumsHit`, `ArtistHit`,
`fm::Hit`, `chrome::StatusTap`). The Library even records its tab strip while drawing it, so a tap
lands on the label the user sees (`lib_tab_zones`) — the core of the design below, in one place.

## The design: three layers

### 1. Palettes — done

Colours from a text file, no code: [`PALETTES.md`](PALETTES.md).

### 2. Skins — one layout contract

A skin is Rust, compiled in and chosen in Settings, and it owns layout. `App` stays the only thing
that decides what happens; a skin draws, and says where each control is while it draws it:

```rust
pub trait Skin {
    fn name(&self) -> &'static str;
    /// Draw one frame of `view`, registering everything tappable as it is drawn:
    ///     ui.hit(rect, Hit::Play);
    ///     ui.slider(rect, SliderId::Balance);
    ///     ui.scroll_extent(content_height);
    fn render(&self, ui: &mut Frame, view: &View);
}
```

- **`View`** — one enum over the per-screen view structs `App::render` already builds. Screens that
  still take positional arguments (Library, Album, Playlist, EQ, USB-DAC, Keyboard, …) get structs
  first.
- **The hit map.** A tap is looked up among the regions the last frame registered and becomes a
  named `Hit` — `Play`, `Row(n)`, `Tab(t)`, `Back`, `Toolbar(dest)`, `Swatch(n)`. `App::on_hit` does
  what `App::tap` does today, without a single coordinate. Drawing a control and making it tappable
  become the same line of code, which ends a class of bug this repository keeps finding: the Menu
  back chevron that answered nothing, the Folders bar reserved and never drawn, row heights drifting
  between the render and the hit test.
- **Drags and scrolling.** A slider registers its rectangle and `App` turns finger position into a
  value. A list reports its content height, and `App` keeps fling, clamping and the scrollbar.
- **Fallback.** A skin implements only the screens it redesigns and inherits the rest from `cinder`.
  A first skin can be a single new Now Playing screen.
- **Safety.** A skin cannot touch the device, but a panic still ends the process, and the device
  reboots into the escape ladder. Hence the contract tests below — and an unknown `skin=` in the
  settings file falls back to `cinder`, the rule `Accent::from_index` already follows.

### 3. The contract tests — what makes it a standard

One suite, run against every registered skin; passing it is what being a skin means:

- nothing drawn off the panel (`tests/ui_overflow.rs` already checks this for one skin);
- every hit region at least 44 px, and no two overlapping;
- a way back from every screen;
- no panic on an empty library, hostile strings, night mode or 140% UI scale;
- every palette that loads still meets its readability rules on the skin's surfaces;
- `player/cinder-host/golden.txt` per skin, so a skin's own changes are reviewed as pixels.

## Not recommended: layouts as data

Layout files (JSON, something QML-like) or scripting (Lua, WASM) would let a skin ship without a
build. On this device that means writing a layout engine — a project of its own — and paying for it
in frame time and battery on the player's Cortex-A7, while render/hit agreement gets harder to keep
rather than easier. Worth revisiting only if people who cannot build Rust turn up wanting to make
skins.

## Designing a skin, once step 2 lands

1. Design at 480 × 800. The June design exploration already holds three complete alternatives —
   Hi-Res, Nocturne and Terminal (`design/handoff_raw/nw-a55/project/screens-*.jsx`).
2. Implement the screens that change, under `player/cinder-ui/src/skins/<name>/`.
3. `cargo run -p cinder-host -- --skin <name>` draws every screen, and
   `cargo run -p cinder-sim -- --skin <name>` clicks through them. *(Both flags arrive with step 2.)*
4. The contract tests pass.

## The order

| Step | What | State |
|---|---|---|
| 0 | A pixel hash for every preview (`player/cinder-host/golden.txt`), so a refactor can prove it moved nothing | **Done 2026-09-11** |
| 1 | Palettes | **Done 2026-09-11**, device-verified the same day |
| 2 | `Frame`, the hit map and `Skin`, with today's screens becoming the `cinder` skin — status bar, Now Playing and Menu first: the smallest surface, and it retires the literal transport circles. Golden hashes must not move | Next |
| 3 | The remaining screens, Library last (~300 references). The coordinate-driven tests keep running against `cinder` throughout | |
| 4 | A first new skin — Terminal, the furthest from Cinder, so it tests the contract hardest | |
