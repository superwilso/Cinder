# Designing for Cinder

This guide is for anyone designing for Cinder, whether that's a palette, a mockup or a whole new
style. It covers the device you are designing for, the parts every screen is built from, and the
rules a design has to follow to work on the player. Each rule links to the code that enforces it,
so the numbers here are the ones the player uses.

**Contents**

1. [The device](#1-the-device)
2. [Three layers: palette, style, accent](#2-three-layers-palette-style-accent)
3. [The screen](#3-the-screen)
4. [Colour](#4-colour)
5. [Type](#5-type)
6. [Touch](#6-touch)
7. [Icons and glyphs](#7-icons-and-glyphs)
8. [Now Playing](#8-now-playing)
9. [Words](#9-words)
10. [Performance](#10-performance)
11. [Contributing a design](#11-contributing-a-design)

---

## 1. The device

| | |
|---|---|
| Panel | 480 × 800, portrait, about 3.1 inches. Everything is designed at 1:1 pixels. |
| Input | **Touch, plus hardware buttons**: play/pause, previous, next, volume up and down, power and the Hold switch. There is **no d-pad and no select key.** A design that needs focus moved by keys cannot be used on this player. |
| Processor | A single Cortex-A7 core does the drawing, with no GPU compositing in the path. Every frame is drawn in software. |
| Where it's used | In a pocket, in the dark at night, at arm's length on a bus. Text has to read in all three. |

The hardware buttons always work, whatever is on screen. Previous and next skip tracks from any
screen, so your design doesn't need to show skip controls anywhere except Now Playing.

## 2. Three layers: palette, style, accent

How Cinder looks is three separate choices. Keep them separate in your design too.

| Layer | What it changes | Who can make one | How it ships |
|---|---|---|---|
| **Palette** | Colours only | Anyone, with a text editor or [Flint](https://github.com/superwilso/flint) | A `.palette` file, shared in [cinder-themes](https://github.com/superwilso/cinder-themes). See [`PALETTES.md`](PALETTES.md) |
| **Style** | Layout, type, the shape of controls | Someone who writes Rust (a designer can hand one over as a mockup) | Compiled in and chosen in **Settings ▸ Display ▸ Style**. See [`PLAN_design_styles.md`](PLAN_design_styles.md) |
| **Accent** | The one colour that means "act here" | Built in (six), or set by a palette | **Settings ▸ Display ▸ Accent** |

**A style never brings its own colours.** It draws with the palette's tokens (§4). That's why every
palette, the six accents and night mode keep working under every style. If your mockup uses a
colour the tokens can't name, either it's a palette (ship it as one) or the design has to change.

Today a style redesigns **Now Playing** and inherits every other screen from Cinder. The other
screens follow the same rules, in the order listed in [`PLAN_design_styles.md`](PLAN_design_styles.md).

## 3. The screen

Every screen is built on the same bands, top to bottom:

```
  0 ┌──────────────────────────────┐
    │ status bar                   │  clock · codec badge · menu · shelf · battery   (44 px)
 44 ├──────────────────────────────┤
    │ header band                  │  back chevron + title, or on Now Playing:     (to 91)
    │                              │  the Menu band, Lyrics chip, sleep badge
 91 ├──────────────────────────────┤
    │                              │
    │ content                      │  lists, controls, the cover
    │                              │
736 ├──────────────────────────────┤
    │ Now Playing bar              │  on screens other than Now Playing              (64 px)
800 └──────────────────────────────┘
```

| Constant | Value | Where |
|---|---|---|
| Status bar height | 44 | `chrome::STATUS_H` |
| Header bottom | 91 | `chrome::HEADER_BOTTOM` |
| Now Playing bar | 64 | `chrome::NP_BAR_H` |
| Side margins | 20 px each side (content from x 20 to x 460) | `kit::LEFT`, `kit::RIGHT` |
| Section label | 34 tall, mono capitals | `kit::SECTION_H` |
| Primary button | 56 tall, full width between the margins | `kit::BUTTON_H` |
| Chip | 44 tall, 8 px apart, equal widths | `kit::CHIP_H`, `kit::CHIP_GAP` |

**The status bar is drawn once, by the app, on every screen.** A design doesn't draw its own clock
or battery. The bottom 28 px of the panel is where a swipe up opens the Shelf. The top edge is
where a pull down opens quick settings, if the owner has turned that on.

## 4. Colour

Design with the **token names**, not hex values. The palette supplies the values.

| Token | Use it for |
|---|---|
| `bg` | The screen background |
| `panel` | Raised surfaces: sheets, toasts, the filter strip, an empty meter's track |
| `line` | Hairlines between rows, around controls, and the unplayed part of a rail |
| `ink` | Primary text: titles, row labels |
| `dim` | Secondary text: artists, subtitles, times |
| `faint` | Tertiary text: captions, section labels, idle icons |
| `acc` | **The accent. It means "this is live, act here":** the play button, the played part of the rail, the selected chip, a switch that's on |
| `acc_ink` | Anything drawn **on** an accent fill: the play glyph, a selected chip's label |
| `row_sel` | The wash behind the highlighted row |

The rules:

- **The accent is for things you can act on, and what's active.** Use it on as few elements per
  screen as possible. Never use it for decoration, headings or plain labels.
- **Text colour follows importance:** `ink`, then `dim`, then `faint`. The palette rules guarantee
  each one stands out more than the next.
- **Night is the same design, dimmer.** Night scales every colour to 55%, and the Brightness row
  scales it further. The cover is dimmed too: Cinder shows it at about a third of its brightness.
  Don't design a separate night look that needs colours of its own.
- **Readability is enforced, not suggested.** The player refuses a palette whose text would be
  unreadable. The contrast floors are in [`PALETTES.md`](PALETTES.md) ("What the player refuses").
  A mockup should pass them with Cinder's palette and with a light one such as Paper.

## 5. Type

Two families, both bundled:

- **Hanken Grotesk** (sans): Light, Regular, SemiBold, Bold and ExtraBold, for titles, labels and prose.
- **JetBrains Mono** (mono): Regular and Bold, for numbers, codecs, times, and the tracked
  capitals used for section labels.

Sizes are **roles**, not numbers ([`scale.rs`](../player/cinder-ui/src/scale.rs)):

| Role | Size | Use |
|---|---|---|
| `TITLE` | 22 | Screen titles, and the big value a screen exists to show |
| `ROW` | 19 | The name of the thing on a row |
| `SECONDARY` | 16 | A value beside a label, the second line under a row |
| `CAPTION` | 13 | Captions, hints, section labels |

Now Playing's title is larger (29 in Cinder, 34 in Nocturne), and the lock-screen clock is larger
still. Those are one-off display sizes, not body sizes.

**Every size grows with the UI scale, from 80% to 140%.** The boxes around the text don't grow.
Design every text box for 140%:

- **The title scrolls (marquees)** rather than being cut off, because the title is the answer to
  "what is this?". Other long text is shortened with an ellipsis.
- **Never place two runs of text on one baseline with fixed x values.** Lay out one against the
  other, the way the artist gives way to the codec on Now Playing.
- **Assume hostile strings:** a 90-character classical title, a Cyrillic or Japanese artist, a
  three-hour track (`2:45:09`). The overflow tests use exactly these.

Mono capitals are **tracked** (letter-spaced about 0.14–0.18 em) and set at 11–13 px. They are
labels, never sentences.

## 6. Touch

- **Every target is at least 44 × 44 px.** A drawn control can be smaller than that (a 24 px
  shuffle icon, a 6 px rail), but the area that answers the tap can't.
- **Targets don't overlap.** Where two have to touch, one of them has to win, and your design has
  to say which.
- **Row heights are fixed per kind of row:**

  | Row | Height |
  |---|---|
  | A track (Songs, an album, Up Next, a playlist, a folder) | 62 (`TRACK_ROW_H`) |
  | A group of tracks (an album, an artist, a playlist) | 68 (`GROUP_ROW_H`) |
  | A setting | 64 (`SETTING_ROW_H`) |
  | A picker | 56 (`PICKER_ROW_H`) |

- **Gestures the whole app already uses.** A design can't give these another meaning:

  | Gesture | Means |
  |---|---|
  | Swipe right from the left edge (starting x ≤ 38) | Back |
  | Swipe up from the bottom edge | The Shelf |
  | Pull down from the top edge (if quick settings is on) | Quick settings |
  | Vertical drag on a list | Scroll, with fling |
  | Horizontal swipe on Now Playing's cover | Turn the page (cover, spectrum, level) |
  | Horizontal swipe below the cover | Skip track |
  | Swipe right on a track row | Add to Up Next |
  | Swipe left on a track row | Play next |

- **Where a control is drawn and where it's tapped come from the same numbers.** In code, one
  function returns every target (`now_playing::layout`), and both the draw and the tap read it. In
  a mockup, mark each target's area, not just its icon.
- **Every screen has a way back.** The left-edge swipe works everywhere. A design should also show
  something to tap: the header's back chevron, or on Now Playing, the Menu band and the toolbar.

## 7. Icons and glyphs

- Icons are drawn as vectors in [`icons.rs`](../player/cinder-ui/src/icons.rs): play, pause, prev,
  next, shuffle, repeat, heart, library, queue, Bluetooth, settings, and others. Use one that
  exists, or specify the new one's shape.
- **Only use characters the two bundled fonts have.** Latin, digits and common punctuation are
  safe, and so are `·`, `—` and `▸`. Avoid decorative Unicode such as `►`, `░`, `█` and emoji.
  - A character the bundled fonts lack sends the player searching Sony's system fonts, which costs
    tens of megabytes.
  - A character no font has can run the player out of memory and restart it.
  - The test `ui_chrome_never_reaches_the_device_font_chain` fails on any such character in the
    app's own text.
  - This is why the Terminal style writes `[<<]` and `> PLAY` rather than `◄◄` and `►`.

## 8. Now Playing

Now Playing is the screen people look at most. It's also the one a style redesigns today.
Whatever it looks like, a Now Playing design has to have:

1. **The status bar**, and **the band under it (y 44 to 91) left clear.**
   - Tapping that band opens the Menu.
   - The Lyrics chip sits at its left when a song has lyrics.
   - The sleep-timer badge sits at its right.
2. **The page block**: the cover, or the spectrum or level page in its place, turned by a swipe.
   It sits below the band and above the rail.
3. **Every control:** like, the title block (a tap opens Track information), a seek rail you can
   drag, elapsed and remaining time, shuffle, previous, play/pause, next and repeat. Repeat has
   four modes: off, one, all and album.
4. **A way to the Library, Up Next, Bluetooth and Settings.** In Cinder that's the bottom toolbar.
5. **A state for "nothing playing"**, which is what the player shows after it starts up.

The rail's grab band is at least 44 px tall, and no other control sits inside it. The cover comes
from the player at 480 × 480. A style can show it smaller, and it is scaled once per track.

### The styles today

| Cinder | Nocturne | Terminal |
|---|---|---|
| <img src="screenshots/now-playing.png" width="190" alt="Cinder style"> | <img src="screenshots/style-nocturne.png" width="190" alt="Nocturne style"> | <img src="screenshots/style-terminal.png" width="190" alt="Terminal style"> |
| Full-bleed cover, a large filled play button, icons in the toolbar | Inset cover, large light title, thin rail, outlined play ring, words in the toolbar | Framed cover, mono capitals, a rail of cells, bracketed text buttons |

The contract every style passes is `now_playing::tests::every_style_keeps_the_contract`.

## 9. Words

- **Sentence case** for titles, labels and buttons: "Nothing playing", "Add to Up Next".
- **Mono capitals** only for section labels, codecs and short status words: `NOW PLAYING`,
  `FLAC 24/96`, `SLEEP 23M`.
- Say what is there, and what to do next when it's empty: "Nothing playing · Choose a track from
  your library". Don't draw an empty box.
- Name things the way the rest of the app does: *Library*, *Up Next*, *Shelf*, *Palette*,
  *Style*, *Accent*.

## 10. Performance

This is what separates a design that looks right in a mockup from one that runs on the player:

- **Nothing is resampled every frame.** Covers are scaled once per track. A cached shape is fine;
  a gradient recomputed 20 times a second isn't.
- **No large allocation while drawing.** The visualiser redraws about 20 times a second, and the
  player has run out of memory before because of repeated allocation in the drawing code.
- **Blur, transparency over large areas, and shadows are expensive.** A translucent veil over the
  cover (the Veil visualiser size) is about the limit.
- **Fixed layout beats measured layout.** A position computed from text width changes with every
  song and every UI scale. Prefer boxes with fixed edges and text that fits or scrolls inside them.

## 11. Contributing a design

**A palette:** follow [`PALETTES.md`](PALETTES.md), or use Flint's palette editor, and share it
through [cinder-themes](https://github.com/superwilso/cinder-themes).

**A mockup** (for a style, or a screen that doesn't exist yet):

1. Draw at **480 × 800**. Use one frame per state: playing, paused, nothing playing, night, 140%
   UI scale, and a long title.
2. Name colours by token (§4) and text by role (§5).
3. Mark every tap target's area, and check each one is at least 44 × 44.
4. Keep the bands in §3 and everything in §8's list.
5. Open an issue on [Cinder](https://github.com/superwilso/Cinder) with the frames attached.

**A style, in code:**

1. Add a variant to `Style` in [`style.rs`](../player/cinder-ui/src/style.rs), with a name, a
   settings word and one line for Settings.
2. Add a layout function and a render function for it to
   [`np_styles.rs`](../player/cinder-ui/src/np_styles.rs). The render draws every control from the
   layout's regions. No coordinate may appear in one function and not the other.
3. Add previews to `cinder-host`: playing, with the Lyrics chip and sleep badge, each page, and
   nothing playing.
4. Run the gates:

   ```sh
   cd player
   cargo test --release                        # includes the style contract and the overflow audit
   cargo run --release -p cinder-host -- --audit
   cargo run --release -p cinder-host -- --bless   # after looking at the new PNGs in player/out/
   ```

5. Open a pull request with the new previews attached.
