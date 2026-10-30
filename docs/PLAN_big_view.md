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

## Final pass (2026-10-25)

The first run reported 121 of 416 previews; the first redesign pass brought that to 103. The
remaining page-specific work is now addressed:

| Page | Big view change |
|---|---|---|
| Pairing | Long scan and connection captions are shortened only in Big view. |
| BT Receiver | The long mode description is shortened, and the experimental footer wraps. |
| Bluetooth ▸ Sound quality | The codec note and Enhanced Mode caption use concise Big-view copy. |
| Sound, Presets, Date & time, Settings, onboarding | Explanatory lines use concise Big-view copy; normal-size copy is unchanged. |
| FM | Fine-tune buttons grow vertically, and rendering and hit testing share the scaled height. |
| Equalizer | Preset hit areas and the drawn band handles grow with Big view. |
| Winamp skin | The existing nearest-control hit test gets a wider Big-view margin; the skin is not redrawn. |
| Cassette tape | Its three transport regions already cover the full sideways screen; the top-right Menu target is 120 × 100 px. |

The latest `--big` pass reports 68 of 422 previews. The remaining reports are predominantly
expected truncation of long song, album, artist and playlist names, long sample device names, and
literal progress ellipses. Review `out/big/` for fixed-height pages that the text audit cannot
measure.

## Done

| Page | What Big view does there |
|---|---|
| Now Playing | In every style, a tap anywhere in the transport row goes to the nearest control (`now_playing::big_targets`, read only by the tap test, so no style's drawing moves); the heart's target is 76 px square. Cinder's style draws its glyphs, heart, seek bar and toolbar icons 1.25x; Nocturne and Clock grow their transport glyphs. The Clock style's time no longer grows with the text size (it was cut to "14:…"). |
| Menu | Its rows were squeezed to fit the glass, so Big view left them at 63 px. Now they keep the full 80, START ON follows them with its chips two to a line ("Now Playing" was cut four across), and the page scrolls under the fixed strip. |
| Display | Scrolls (the Style chips ran off the bottom). |
| Bluetooth | Shows four paired devices instead of five (four Big view rows are the height of five), so THIS DEVICE no longer runs into the footer; the NFC hint gets its own line above RECEIVER MODE. |
| Lists with covers | Albums, Artists, Playlists, an artist's albums, a playlist's songs and Up Next draw their covers at 60 px instead of 48, and the text beside them moves over by the difference. The art cache keeps only 48 px thumbnails, so each is scaled up once and kept (64 at most, about 0.7 MB). *Device-unverified.* |
| Grid covers | The Albums, Artists and Playlists grids draw covers at 132 px instead of 96, three across, scaled once from the 96 px cover and kept. *Device-unverified.* |
| Lock | The footer splits at its last " · " onto two lines whenever it does not fit, at any size. |
| Intro pages | Each key sits above its description instead of in a 130 px column beside it. |
| Soundscapes | The level's name sits above a full-width slider. |
| SensMe (empty) | The explanation wraps to the glass (it was three hand-broken lines). |

The pull-down panel, Library, albums, artists, playlists, Settings, Sound, Device, Storage, SensMe
channels and BT Receiver already worked at Big view.

## Gate

When a page is fixed, its previews stop appearing in `--big`'s list. Six Big view previews are in
the golden set (`big_*`: Now Playing, Library, Settings, the Menu and Display scrolled to the end,
Bluetooth), so Big view cannot quietly break again.
