# Palettes

A palette is a text file that recolours Cinder: the six neutral colours for day and for night, and —
only if it needs one — an accent of its own. Layout, type and icons stay exactly as they are;
changing those is what skins are for ([`PLAN_skins.md`](PLAN_skins.md)).

The rules below are enforced by [`player/cinder-ui/src/palette.rs`](../player/cinder-ui/src/palette.rs),
and its tests hold the example files to them.

*Status 2026-09-11: verified on the player — Slate and Paper loaded, a broken file skipped with its
reasons logged, the choice kept across restarts ([`DEVICE_TESTS.md`](DEVICE_TESTS.md), "RESULTS
2026-09-11 (evening)").*

## Try one

1. Copy [`player/cinder-ui/palettes/cinder.palette`](../player/cinder-ui/palettes/cinder.palette) —
   Cinder's own colours written out — to, say, `mine.palette`. The file name is the palette's id:
   lowercase letters, digits, `-` and `_`. `cinder` is taken by the built-in.
2. Change some colours.
3. On a PC, check it the way the player will and look at every screen in it:

   ```sh
   cd player && cargo run -p cinder-host -- --palette ../mine.palette
   ```

   It either prints exactly what the player would say when refusing the file, or draws all 228
   preview screens into `player/out/palette_mine/`.
4. Connect the player over USB, make a folder called **`cinder_palettes`** in the root of its storage
   (next to `cinder_settings.conf`), copy the file in, and unplug.
5. **Settings ▸ Palette** — each tap steps to the next palette, then back round to Cinder.

The folder is read at boot and again whenever Settings opens, so there is no reboot to wait for. The
choice is saved like any other setting; Settings ▸ Reset settings goes back to Cinder.

## The keys

Every key is optional. A key left out keeps Cinder's value.

| Key | What it colours |
|---|---|
| `name` | What Settings shows. 1–16 characters; the id if there is none. |
| `day.bg` | The screen background. |
| `day.panel` | Raised surfaces: the filter strip, toasts, sheets. |
| `day.line` | Hairlines between rows and around controls. |
| `day.ink` | Primary text — titles and row labels. |
| `day.dim` | Secondary text — artists and subtitles. |
| `day.faint` | Tertiary text — mono values, captions, section headings. |
| `night.bg` … `night.faint` | The same six, for night mode. |
| `day.accent` | The one saturated colour: the selected label, the progress fill, the shuffle band, the play button. |
| `day.accent_ink` | What is drawn **on** an accent fill — the play glyph, the shuffle band's text. |
| `day.row_select` | The wash behind the highlighted row. |
| `night.accent`, `night.accent_ink`, `night.row_select` | The same three, for night. |

Colours are `#rrggbb`; the `#` is optional. A line starting with `#` is a comment.

### Night values are written before night mode dims them

Night mode scales every night colour to 55% on the way to the panel — except `accent_ink`, which is
already the dark half of its pair — and at night the Brightness row scales further still. Write
night values the way `theme.rs` writes Cinder's: `night.ink = #8d8170` reaches the panel as
`#4d463d`. That is why Cinder's own numbers, copied into a file, reproduce Cinder exactly.

### Accents: leave all six out, or set all six

Leave the accent keys out and **Settings ▸ Accent** works as it always has. The palette is then
checked against all six of Cinder's accents, because the picker offers every one of them — which
suits any dark palette.

A light palette has to bring its own. Cinder's accents were tuned against near-black, and the light
ones all but vanish on a light background, so the player refuses a light palette that leaves the
accent to the picker and says why. With all six keys set, the palette's accent is used in both
modes, the Accent row reads **SET BY PALETTE**, and a tap on it says the same.

## What the player refuses

A palette that would be hard to read is **not loaded**. The reason goes to `cinderhome.log` in the
root of the player's storage, and Settings ▸ Palette says how many files were skipped. This player
has one screen and no other way in — a palette with unreadable text would leave nothing to read your
way back out with — so these rules are not a style guide. They sit a margin under what Cinder itself
measures and turn away only the unreadable.

Contrast is WCAG's ratio, from 1 (none) to 21 (black on white), measured on the colours as they reach
the panel:

| | Day | Night |
|---|---|---|
| `ink` on `bg`, and `ink` on `panel` | ≥ 4.5 | ≥ 1.9 |
| `dim` on `bg` | ≥ 3.0 | ≥ 1.35 |
| `faint` on `bg` | ≥ 1.8 | ≥ 1.12 |
| `accent` on `bg`, `accent_ink` on `accent`, `accent` on `row_select` | ≥ 3.0 | ≥ 1.2 |
| *Cinder itself: ink / dim / faint* | *15.9 / 6.2 / 2.9* | *2.26 / 1.54 / 1.25* |

And:

- `ink` has to stand out more than `dim`, and `dim` more than `faint`.
- `night.bg` has to stay dark: relative luminance at most 0.02 (about `#272727`) after dimming.
- Night's floors are far under WCAG's on purpose. Night mode exists to emit as little light as a
  readable screen can, and Cinder's own night text sits at 2.26.

Also refused, each with its line number where there is one: an unknown key (a typo would otherwise
fall back to Cinder silently), a key set twice, a colour that is not `#rrggbb`, some but not all of
the accent keys, a file over 16 KB, and an id that is not lowercase letters, digits, `-` and `_`. At
most 32 palettes load; any more are listed as skipped. macOS's `._name.palette` metadata files are
ignored.

## The examples

| File | |
|---|---|
| [`cinder.palette`](../player/cinder-ui/palettes/cinder.palette) | Cinder's own colours — the template. It will not load under that name; rename the copy. |
| [`slate.palette`](../player/cinder-ui/palettes/slate.palette) | Cool blue-greys. Keeps the accent picker. |
| [`paper.palette`](../player/cinder-ui/palettes/paper.palette) | Dark ink on off-white by day, with its own burnt-orange accent; Cinder's night after dark. |
