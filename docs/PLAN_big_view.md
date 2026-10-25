# Big view

Settings ▸ Big view: bigger text and easier-to-press controls, in one switch, off by default. For
anyone who finds Cinder's rows and buttons small. The panel is about 11.9 px per mm, so a 62 px
track row is 5.2 mm tall; the usual guidance for a finger is 7 to 9 mm.

## What one switch does

- **Text** goes to 130 % (`nav::BIG_TEXT_PCT`) if it was smaller. The Size slider still goes to
  140 %. Turning Big view off puts the text back to 100 % only if it is still at the 130 % Big view
  chose.
- **Every list row** is 25 % taller (`scale::LARGE_PCT`): track rows 62 → 78 px (6.5 mm), setting
  rows 64 → 80, album and artist rows 68 → 85, picker rows 56 → 70. The four row heights in
  `scale.rs` are functions of the switch, and every layout constant built on them became a function
  too, so the drawing and the hit test of every list still read one number.
- **Back, Sort and the library tabs** grow with `text::touch_scale()`, which Big view raises to at
  least 1.25.
- With Big view off, nothing moves: all 416 golden previews are byte-identical to before.

## How to see it

`cargo run -p cinder-host --release -- --big` renders every preview in Big view to `out/big/` and
lists, per preview, text cut short, text past the edge, and text that runs into or under other text.
Song and album names cut short in a list are expected (they are cut at any size); everything else
on that list is a page still to fix. Fixed-height pages that run off the bottom are not detected
yet. Look at their PNGs.

## Pages still to redesign, as of the first run (2026-10-25)

121 of 416 previews report something; most are long song titles. What is left is these pages, in
the order a user meets them:

| Page | What breaks in Big view | Direction |
|---|---|---|
| Now Playing | ⏮ ⏯ ⏭, shuffle, repeat, the like heart and the bottom bar keep their size; the Clock style's time is cut | Grow the transport row and spread it edge to edge; a bigger seek bar handle and touch band |
| Display | Runs off the bottom: the Style chips are cut | Make it scroll, like Settings |
| Bluetooth | Runs off the bottom; the Debug log sub line runs into the NFC hint and Receiver mode | Make it scroll; move the NFC hint into the strip |
| Lock | The footer ("HOLD LOCK · SIDE KEYS ACTIVE…") runs past both edges | Two lines, or drop the middle part |
| Intro (Buttons, Gestures, Features) | Labels run into their descriptions; cut descriptions | One column: label above description |
| Soundscapes | "On its own" and "With music" labels cut to "On its o…" | Label above the slider, not beside it |
| Pairing, BT Receiver | Captions and the EXPERIMENTAL footer cut | Wrap the captions |
| Sound (Tone Control note), Presets, Volume curve, Date & time | One sub line each is cut | Wrap, or shorter copy |
| SensMe (empty) | Something draws far past the right edge | Find and fix |
| FM, Equalizer | Not broken, but the ±0.1, ◀ ▶ buttons and the EQ handles stay small | Bigger buttons and handle bands |

The pull-down panel, Library, albums, artists, playlists, Settings, Sound, Device, Storage, Menu,
SensMe channels and BT Receiver already work at Big view.

## Gate

When a page is fixed, its previews stop appearing in `--big`'s list. A few Big view previews
(Settings, Library, Now Playing) should then join the golden set, so Big view cannot quietly break
again.
