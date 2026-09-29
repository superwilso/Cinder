# Plan — design styles

*Written 2026-09-29. Step 1 is built and host-tested; the rest is the order of work. The design
rules every style follows are in [`DESIGN_GUIDE.md`](DESIGN_GUIDE.md).*

A **style** is a way of laying out and drawing Cinder's screens: layout, type and the shape of
controls. It sits between the palette (colours, a text file) and the accent (one colour, built
in). It is Rust and compiled in, which is the 2026-09-14 decision ("skins only",
[`PLAN_2026-09-14.md`](PLAN_2026-09-14.md) Part C) carried out. This plan replaces steps 2–4 of
[`PLAN_skins.md`](PLAN_skins.md), and its step 1 is also step 1 of
[`PLAN_now_playing_layouts.md`](PLAN_now_playing_layouts.md).

## What is built (step 1)

| Piece | Where |
|---|---|
| `Style` (Cinder, Nocturne, Terminal), with names, settings words and one line each | `player/cinder-ui/src/style.rs` |
| The style rides on `Theme`, so every render already receives it | `theme.rs` |
| Now Playing's layout as a pure function. `layout(style, night, lyrics)` returns every target (`Hit`), its shape, the seek rail and the page block | `now_playing.rs` |
| `nav` asks that layout for taps, the rail and the page swipe. No coordinate for Now Playing is left in `nav.rs` | `nav.rs` |
| Cinder's layout reproduces today's screen. Every golden hash of the Cinder Now Playing is unchanged. The literal transport circles are gone, and Prev and Next now answer where they are drawn (128 and 352, not 130 and 350) | `now_playing::cinder_layout` |
| Nocturne and Terminal: a layout and a render each, the render drawing every control from the layout | `np_styles.rs` |
| An inset cover is scaled once per track, into a single-entry cache (about 480 KB while such a style is on) | `art::draw_fitted` |
| **Settings ▸ Display ▸ STYLE · NOW PLAYING**: three chips, and Select steps through them | `display.rs` |
| `style=` in `cinder_settings.conf`. An unknown word keeps the current style | `cinder-ffi` |
| 16 golden previews, and the Nocturne and Terminal screenshots | `cinder-host`, `docs/screenshots/` |

**The contract** (`now_playing::tests::every_style_keeps_the_contract`). For every style, day and
night, with and without lyrics:

- every control Cinder has is present, including Play and a way out;
- every target is on the panel, at least 44 px each way, and answers at its own middle;
- the rail's grab band is at least 44 px tall, holds the rail, and has no other control in it;
- the page block sits above the rail;
- for a new style only: no two targets overlap, and nothing but the Menu and the Lyrics chip is in
  the band under the status bar.

`ui_overflow` adds that every style stays on the panel with its text apart, at every UI scale with
hostile strings, and that its labels never reach Sony's font chain. `nav` tests that every style is
tapped, scrubbed and swiped where it draws.

## Next

| # | What | Effort | Gate |
|---|---|---|---|
| 2 | **Device run**: [`DEVICE_CHECKLIST.md`](DEVICE_CHECKLIST.md) §23 | One session | §23.1–23.6 |
| 3 | **The kit, per style.** `kit::row`, `section_label`, `chips` and `primary_button` ask the theme's style how to draw. One change restyles Settings, Display, Sound, Bluetooth and every other settings screen at once, because they are all built from the kit | 1–2 days | Golden unmoved for Cinder; contract: row and chip targets unchanged, since the kit's geometry is shared |
| 4 | **The header and the Now Playing bar, per style.** `chrome::header` (back chevron, title) and the bar at the foot of every screen. The status bar stays the same in every style | 1 day | The same |
| 5 | **Lock screen and Menu** | 1 day | A contract like Now Playing's: every target ≥ 44 px |
| 6 | **Library rows and grid, per style.** Row heights stay fixed (the guide's §6); a style draws inside them. That keeps the ~300 hit-test references in `nav.rs` valid | 2–3 days | Golden unmoved for Cinder; hit tests unchanged |
| 7 | **Share styles as designs.** A `styles/` folder in [cinder-themes](https://github.com/superwilso/cinder-themes) for mockups made to the guide, and a "Propose a style" issue form. Accepted ones are built as Rust | ½ day | — |
| 8 | **Layout files for Now Playing**: only if the owner accepts the exception in [`PLAN_now_playing_layouts.md`](PLAN_now_playing_layouts.md) §1. `Layout` is already the data such a file would fill, and the contract is already the check it would get | 3–4 days | That plan's §6 |

Steps 3 and 4 are the best value next: together they change the look of every screen except the
Library.

## What a style may not do

- Bring colours. It draws with the palette's tokens. That is what keeps night mode, the accent
  and every shared palette working ([`DESIGN_GUIDE.md`](DESIGN_GUIDE.md) §2).
- Move the status bar, or put its own controls in the band under it.
- Change what a gesture means (the guide's §6 table).
- Use a character the bundled fonts lack.
- Allocate or resample per frame.

## Risks

- **Memory.** The inset cover's cache is one image, about 480 KB. A style that wanted two cover
  sizes on one screen would double that. The contract does not check this; review does.
- **Two styles drift.** A fix to Cinder's Now Playing (a new control, say) must reach every style.
  The contract's first rule, "every control Cinder has", fails until it does. That is intended.
- **Scope.** Each screen a style redraws adds preview surface: 8 previews a style for Now Playing.
  The kit (step 3) is the cheap way to widen a style. Screen-by-screen is the expensive way.
