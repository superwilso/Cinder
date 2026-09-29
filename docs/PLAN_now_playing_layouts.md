# Plan — custom Now Playing screens that people can make and share

*Written 2026-09-29. One decision is the owner's (§1); the rest is the order of work once that is
made.*

> **Step 1 (§3, §6) is built**, as part of [`PLAN_design_styles.md`](PLAN_design_styles.md): Now
> Playing's layout is a pure function (`now_playing::layout`) that the draw and the tap both read,
> and two compiled-in styles use it. A layout file would fill the same `Layout` and pass the same
> contract. The §1 decision is still open.

The request: a way for people to **make** their own Now Playing screen and **share** it, the way
palettes are shared now. Palettes are text files that anyone can write, check on a PC, drop in a
folder, and pick in Settings ([`PALETTES.md`](PALETTES.md)). The goal is the same experience for the
screen people look at most.

---

## 1. The decision this needs first

On 2026-09-14 the owner confirmed **"skins only"**: a new look is Rust, compiled in, and a data
format for layouts is out of scope ([`PLAN_2026-09-14.md`](PLAN_2026-09-14.md) Part C,
[`PLAN_skins.md`](PLAN_skins.md) "Not recommended: layouts as data"). The reasons were real. A
general layout engine is a project of its own. It costs frame time on a Cortex-A7. It makes it
harder for the render and the hit test to agree.

A **shared** Now Playing screen cannot be Rust. A screen shared as Rust reaches other people only
when it is merged and released. Nobody could try one from a forum post. So this request reopens that
decision, but only for a narrow case.

**Recommendation: a bounded layout file for Now Playing only.** It is not a layout engine. It is a
fixed catalogue of the components Now Playing already draws. A file may place, size and style those
components within limits, and nothing else. The limits are the point. They keep the renderer to one
known-cost code path, and they let the player refuse a bad file the same way it refuses an
unreadable palette. Every other screen stays "skins only".

| | Layout file (recommended) | Rust skin only (the 09-14 rule) |
|---|---|---|
| Who can make one | Anyone with a text editor, or Flint's editor | Someone who builds Rust |
| How it is shared | A file: the community repo, a post, a USB copy | A pull request and a release |
| What it can change | Where and how big the fixed components are, and some style choices | Anything |
| Risk to the player | Validated at load; a bad file is skipped with the reason logged | Reviewed in the PR |
| Cost to build | ~5–7 days, shown in §6 | Skins steps 2a–4, which were already planned |

The two paths do not compete. Tier B is still the answer for a design the catalogue cannot express,
and step 2c of the skins plan is the foundation for both (§3).

**Owner: accept the Now-Playing-only exception, or keep Rust skins as the only path.** The rest of
this document assumes the exception.

---

## 2. What a layout file looks like

A layout is a text file, `<id>.layout`, in `cinder_layouts/` next to `cinder_palettes/`. Its syntax
is the palette's syntax: `key = value`, `#` comments, and lowercase ids. Anyone who has written a
palette has already learned it.

```ini
# big-art.layout — the cover as large as the panel allows, the controls tucked under it
name   = Big Art
format = 1

art.x = 0
art.y = 34
art.size = 480            # 480 | 400 | 320 | 240 — sizes the shell can pre-scale once per track
art.corner = 0            # 0–24 px

title.y = 540
title.size = title        # a scale.rs role: title | row | secondary
title.align = left        # left | center
artist.y = 568
album.show = no

rail.y = 612              # the seek rail; its grab band is derived, never written
time.style = split        # split | remaining | none

transport.y = 692         # prev / play / next, spaced as today
transport.extras = yes    # shuffle and repeat at the ends

heart.show = yes
heart.x = 432
heart.y = 548

badge.show = yes          # the codec line
toolbar.show = yes        # library · queue · bluetooth · settings
viz.region = under-art    # off | over-art | under-art — the visualiser styles are unchanged
```

**The catalogue** is what Now Playing already draws, and nothing more. The cover, title, artist,
album, codec line, seek rail, times, transport row, like heart, toolbar, visualiser, lyrics chip and
page dots. Each has the keys that make sense for it and no others. A position is in pixels on the
480 × 800 panel, snapped to a 2 px grid. A text size is a `scale.rs` role, never a number, so the
140% UI scale still works. Colours come from the active palette, never from the layout. A layout
and a palette are two independent choices, and every palette's readability rules still hold.

**Deliberately not in format 1:** images and fonts of the layout's own, arbitrary text, animation,
scripting, and other screens. Pages 2 and 3 (spectrum and level) keep their own layout and inherit
only the positions of the controls. The night theme uses the same file. Cinder's own night header
stays a built-in, because a layout file expressing it would need conditional keys, and format 1 has
none.

**The built-in screen becomes a file.** `cinder.layout` reproduces today's Now Playing exactly, and
a test holds it to that: the golden hashes must not move when the built-in is drawn from it. This is
the same trick `cinder.palette` uses, and it is the test that the catalogue is complete.

---

## 3. What has to be true in the code first

Now Playing's geometry is half named constants and half literals today. `RAIL_Y`, `TOOLBAR_CX`,
`HEART_CX` and `INFO_TOP` live in `now_playing.rs`. The transport row's targets are circles written
straight into `nav.rs`: `hit(240, 692, 44)`, `hit(130, 692, 34)` and so on. A layout file can only
move a control if **one value** decides both where it is drawn and where it is tapped.

That is step **2c** of the skins plan ([`PLAN_2026-09-14.md`](PLAN_2026-09-14.md) C2–C3), and Now
Playing was already first in its order:

```rust
/// Pure: no canvas, no paint order. `render` and `App::tap` both call it.
pub fn layout(np: &NowPlaying, l: &Layout) -> LayoutMap;   // regions: Vec<(Rect, Hit)>
```

- `Hit` names every Now Playing target: `PlayPause`, `Prev`, `Next`, `Shuffle`, `Repeat`, `Like`,
  `Info`, `Lyrics`, `Toolbar(n)`, `Seek`, `Menu`.
- `App::tap` on Now Playing becomes `layout(..).hit(x, y)` → `on_np_hit(Hit)`. **No coordinates are
  left in `nav.rs` for this screen.**
- Gate: every golden hash stays the same, and every existing Now Playing test stays green.

This step is worth doing even if §1 is decided the other way. It retires the literal circles, and
it is the first move of the skins plan that was already agreed.

---

## 4. What the player refuses

A layout is checked at load, like a palette. A layout that fails is **not loaded**. The reason goes
to `cinderhome.log`, and the picker lists the file under SKIPPED with the first thing wrong with it.
The rules come from the skins contract suite ([`PLAN_2026-09-14.md`](PLAN_2026-09-14.md) C4), and
most of them are assertions on the `LayoutMap` with no pixels drawn:

1. **Everything is on the panel**, below the status strip (34 px) and above the bottom edge.
2. **Every tap target is at least 44 px** in both directions. Targets never overlap. The rail's
   grab band counts as a target.
3. **Play/pause is always present.** A way off the screen is always present: the toolbar, or the
   Menu band under the status strip if the toolbar is hidden. It is refused if neither is present.
4. **Text fits at 140% UI scale.** Checked the way `cinder-host --audit` does. Text that would be
   cut short is refused, because a layout that works at 100% and breaks at 140% has not been
   tried at 140%.
5. **Housekeeping**, as for palettes: an unknown key or a key set twice (each with its line number),
   `format` missing or newer than the player, a file over 16 KB, or a bad id. At most 32 layouts
   load.

**If the app still dies with a layout selected** (a bug no rule anticipated), the skins plan's C5
applies. Two crashes with layout X selected revert to `cinder`, log why, and show a note in Settings.
A cosmetic file must never send the player back to Sony's firmware.

---

## 5. Making and sharing

| Step | Where | How |
|---|---|---|
| Make | A text editor, starting from `cinder.layout` | Change numbers, save as `mine.layout` |
| Check | PC: `cargo run -p cinder-host -- --layout mine.layout` | Prints exactly what the player would refuse, or draws Now Playing with it: day, night, 140%, a long title and no cover |
| Check (no Rust) | Flint ▸ **Layouts** | The same checks, compiled in (Flint already carries Cinder's palette rules), with a live preview |
| Try | The player | Copy to `cinder_layouts/`; **Settings ▸ Display ▸ Now Playing layout** lists it with a thumbnail |
| Share | The community repo, `superwilso/cinder-themes`, under `layouts/` | The same route as palettes: a pull request, or an issue form for people without git. CI runs the player's checks and renders the preview PNGs with `cinder-host`, so the gallery shows each file as the player draws it |
| Get | Flint ▸ Layouts ▸ **Browse** | Downloads from the repo's index into the folder Flint sends from |

Rendering the gallery with `cinder-host`, the same code the player runs, is the reason the gallery
can be trusted. A screenshot someone took by hand would show what they meant, not what the file
does.

---

## 6. Order and effort

| # | What | Effort | Gate |
|---|---|---|---|
| 0 | **Owner decision (§1)** | — | — |
| 1 | Now Playing `LayoutMap` + `Hit` (skins 2c for this one screen) | 1–1½ days | Golden hashes unmoved; every existing Now Playing tap test green |
| 2 | `Layout` struct, parser and validator; today's screen as `cinder.layout`; the renderer reads positions from `Layout` | 2–3 days | Golden hashes unmoved when drawn from `cinder.layout`; one refusal test per rule in §4 |
| 3 | The folder, the Settings picker with thumbnails, the crash revert, and `lib`-style persistence (`np_layout=` in `cinder_settings.conf`) | 1 day | nav tests; `cinder-host` previews of the picker |
| 4 | Two example layouts that are really different, e.g. *Big Art* and *Text First* (no cover, large type: the Terminal design's Now Playing), plus `docs/LAYOUTS.md` | ½ day | Both pass the checks; previews in the docs |
| 5 | `cinder-host --layout`; the community repo's CI job and gallery | ½ day | A broken layout in a PR fails CI with the player's message |
| 6 | Flint ▸ Layouts: check, preview, send, browse | 2–3 days | Flint's own tests; its preview matches `cinder-host`'s pixels |
| 7 | Device run: the picker, a switch mid-song, the crash revert, and frame time on the cover page | One session | New `DEVICE_CHECKLIST.md` section |

Steps 1–5 are about 5–6 days and need no device until step 7. Step 6 can follow later. The files
work without Flint.

## 7. Risks worth naming now

- **Cover sizes.** The shell pre-scales one full-size cover per track (`art_full`, 480 × 480).
  Format 1 allows only sizes it can scale **once per track change** (480, 400, 320 and 240). A
  per-frame resample of a 1,425 px JPEG-sourced cover is what made the art cache necessary in the
  first place (`art_cache.rs`).
- **The Now Playing pages.** The spectrum and level pages fill the art block. If a layout moves the
  art block, those pages move with it, and the pager's swipe band follows `art`, not
  `PAGE_TOP`/`PAGE_BOT`.
- **Tests that tap literal coordinates.** Step 1 keeps them working because the built-in layout
  keeps today's geometry. They must never be rewritten to follow a layout. They are the check that
  the built-in did not move.
- **Scope creep toward a layout engine.** Every future key request should be weighed against §1's
  reason for refusing layouts in general. Format 2 is a new decision, not a default.
