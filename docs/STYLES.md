# Making a style

A **style** changes how Cinder is laid out and drawn: type, shapes, how a row or a switch looks,
and Now Playing's whole screen. It's Rust, compiled into the player, and chosen in
**Settings ▸ Display ▸ Style**. There is no runtime UI framework: a style is a few plain drawing
functions over a fixed canvas, which is what keeps the player fast on a 1 GHz Cortex-A7.

Read [`DESIGN_GUIDE.md`](DESIGN_GUIDE.md) first. Its rules apply to every style, above all: a style
**never brings its own colours** (it draws with the palette's tokens), it leaves the status bar
alone, and every tap target stays at least 44 × 44 px.

## What a style can change

Everything a style can change is one trait, `Look`, in
[`player/cinder-ui/src/style.rs`](../player/cinder-ui/src/style.rs). Every method has Cinder's own
drawing as its default, so a style overrides only what it redesigns.

| Method | What it draws | Where it shows |
|---|---|---|
| `np_layout`, `np_render` | Now Playing: where every control is, and the screen | Now Playing |
| `row` | A list row (usually through `kit::row_dressed` with your own `RowDress`) | Every settings screen, Bluetooth, Wi-Fi, playlists |
| `section_label` | A section label, with an optional action on the right | The same |
| `chips` | A row of choices | Display, the Menu, the pull-down panel, the DAC EQ, the receiver |
| `switch` | An on/off switch | Rows with a switch |
| `primary_button` | The screen's one main action | SensMe today; new screens use it |
| `strip` | A one-line state strip under a header | Bluetooth, Wi-Fi, internet radio, device, storage |
| `header_title`, `back_glyph` | The header's title type and its back mark | Every screen with a header |
| `np_bar` | The Now Playing bar at the foot of browsing screens | Library, Settings, everything with the bar |
| `lib_type` | The (family, weight) of the Library's list text; the size stays | Songs, Albums, Artists, Playlists, album/artist/playlist pages |
| `lib_tabs` | The Library's tab strip | The Library |
| `lib_sel`, `lib_rule`, `lib_playing` | A list row's chosen mark, the rule under it, the playing mark | Every Library list and page |
| `lib_band`, `lib_side_button` | The shuffle or Play band, and the square button beside it | The Library, album, artist and playlist pages |

What stays the same in every style: the status bar, the Library's covers and scrollbar, and screens
that draw their own controls (Sound's A/B switch and paths, for example). The Library's grid
captions, group headings, A–Z rail and empty state follow `lib_type`.

The Library is the screen that scrolls, so its hooks are cheap by rule: `lib_type` is a lookup per
row, and nothing a style draws in the Library may cost more per row than Cinder's own drawing (no
dotted rules between rows: a dotted line is 240 single-pixel fills). `render_bench` times the Songs
list in every style:

```sh
cargo test -p cinder-ui --release --test render_bench -- --ignored --nocapture
```

## The one rule: draw inside the boxes

Every box a finger can land on is fixed and shared with the tap test: `kit::ROW_H()`,
`kit::CHIP_H` and `kit::chip_span`, `kit::BUTTON_H`, `kit::SWITCH_W` × `kit::SWITCH_H`,
`chrome::np_bar_rect()` and `chrome::NP_BAR_PLAY_W`. A style changes what is drawn **inside** them
and returns the same y below. It never moves a box, so `nav.rs` never has to know which style is
on, and a style can't draw a control in one place and answer for it in another.

Fit text into a box with `widgets::fit`; never size a box from its text. The UI-scale slider grows
the type, not the boxes.

Now Playing is the exception that proves the rule: a style may move its controls, but only by
returning them from `np_layout`, which the tap test reads too.

## Adding one

1. **Copy a style file.** [`styles/terminal.rs`](../player/cinder-ui/src/styles/terminal.rs) dresses
   everything; [`styles/clock.rs`](../player/cinder-ui/src/styles/clock.rs) is the smallest (it
   reuses Nocturne's layout and kit). Rename the unit struct, its `name`, `token` and `about`.
   - `token` is what `cinder_settings.conf` stores (`style=…`). Lower case; never change it once a
     release has shipped it.
   - Write `about` as `crate::i18n::text("…")` so the translation tools find it.
2. **Register it** in `style.rs`: a variant in `Style`, its place in `Style::ALL`, and one line in
   `Style::look`. That's the only file that names every style.
3. **Override what you redesign.** To dress the kit like another style, put
   `kit_from!(super::terminal::Terminal);` (or `nocturne::Nocturne`) in your `impl Look`. For rows,
   make a `kit::RowDress` (title and subtitle type, the chosen row's mark, the rule, the "opens"
   mark) and pass it to `kit::row_dressed`: the trail, the fitting and the boxes stay shared.
4. **Look at it.** Add your style to the style sweep in
   [`player/cinder-host/src/main.rs`](../player/cinder-host/src/main.rs) (search for
   `STYLE sweep`) and, for Now Playing, the per-style previews above it. Then:

   ```sh
   cd player
   cargo run --release -p cinder-host      # writes every preview to player/out/
   ```

5. **Run the gates.** They cover every style in `Style::ALL` without being told about yours:

   ```sh
   cargo test --release -p cinder-ui       # the Now Playing contract, and ui_overflow:
                                           # every screen in every style, every UI scale,
                                           # hostile strings, text on the panel and apart
   cargo run --release -p cinder-host -- --check   # Cinder's own previews must not move
   cargo run --release -p cinder-host -- --bless   # after looking at your new PNGs
   ```

6. **Open a pull request** with the new previews attached.

## Limits worth knowing

- **ASCII for your own glyphs.** A character the bundled fonts lack falls back to Sony's font
  chain, which costs about 80 MB and has run the player out of memory. Draw rules and marks as
  pixels (`fill_rect`, the dotted helpers in `styles/mod.rs`), not box-drawing characters.
- **Nothing per frame that allocates or resamples.** Some screens redraw 20 times a second. A
  cover is scaled once per track (`art::draw_fitted`).
- **One accent thing per screen,** more or less. The selection mark, the primary button and the
  playing state are where it belongs.
- **A style that differs only in colour is a palette,** not a style: see [`PALETTES.md`](PALETTES.md).

## The styles today

| Style | Now Playing | Kit, header and bar |
|---|---|---|
| Cinder | Full-bleed cover, big controls | Washes, hairlines, chevrons (the default) |
| Nocturne | Inset cover, light type, outlined play ring | Light type, inset rules, pill switches, underlined chips, an outlined button; the Library in light type with outlined bands |
| Terminal | Framed cover, mono capitals, a rail of cells | Mono type, dotted rules, a `>` cursor, bracketed chips and button, a bar of cells; the Library in mono with a bracketed tab |
| Cassette | Reels that wind, piano keys | Terminal's |
| Clock | A large clock where the cover was | Nocturne's |
| Winamp | A `.wsz` skin from the drive | Terminal's |
