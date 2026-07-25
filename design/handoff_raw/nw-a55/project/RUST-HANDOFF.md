# NW-A55 "Cinder" — Rust Implementation Handoff

Companion to the interactive mock **`NW-A55 Final.html`** (open it to see
every screen; all values below match it exactly). Target hardware: Sony
NW-A55 — 3.1" touchscreen, **480×800 portrait**, physical side keys
(power, vol±, prev, play/pause, next), NFC on rear panel.

## 0. Stack recommendation

Battery-first. The whole design uses ONLY: solid-fill rects, 1px lines,
single-color glyph icons, text, and one optional bar visualizer. No
gradients, no blur, no shadows, no transparency compositing required
(the two overlays below can be solid-color substitutes).

- Recommended: **Slint** (slint-ui) with the software renderer, or
  `embedded-graphics` straight onto the framebuffer.
- Render on demand only (dirty-flag). The only animated surfaces:
  visualizer bars (≤15 fps is fine), progress bar (1 fps), volume
  overlay (appears/expires). Everything else is static between inputs.
- Night mode exists to darken the (LCD) panel and minimize wakeups —
  suppress the visualizer's frame rate further there (≤5 fps) or drop it.

## 1. Design tokens

### 1.1 Color — Day theme
| token   | hex       | use |
|---------|-----------|-----|
| bg      | `#0d0c0b` | screen background |
| panel   | `#13110f` | cards, sheets, status overlays |
| line    | `#221f1b` | ALL hairlines/borders (1px) |
| ink     | `#ece7df` | primary text |
| dim     | `#95908a` | secondary text |
| faint   | `#5f5a52` | tertiary text, disabled, captions |
| acc     | `#f4651f` | accent (matches orange chassis) |
| accInk  | `#1a0a02` | text/icons ON accent fills |

### 1.2 Color — Night theme
Pure-black bg; every role is a low-luminance warm tone. Accent =
day accent × 0.55 luminance (`#863810` ≈ ember).
| token | hex | | token | hex |
|---|---|---|---|---|
| bg | `#000000` | | ink | `#8d8170` |
| panel | `#0a0908` | | dim | `#5b5347` |
| line | `#161310` | | faint | `#3b362d` |
| acc | `#863810` | | accInk | `#000000` |

Night also: album art rendered at 30% opacity (or pre-darkened),
full-bleed art replaced by 92px thumbnail on Now Playing.

### 1.3 Type
Two families only:
- **Sans** — Hanken Grotesk (UI text). Weights: 400/600/700/800.
- **Mono** — JetBrains Mono (ALL data: times, codecs, captions,
  status bar, frequencies). Weights: 300/400/700.

Scale (px): 9/10/11 mono captions+data · 12–13 small UI · 15 list
titles · 17 menu rows · 21–23 card titles · 26–28 screen/track titles ·
86–88 mono display (clock, FM freq).
Caption style: mono, uppercase, letter-spacing 0.14–0.22em, `faint`.

### 1.4 Metrics
- Screen padding: **22px** sides (24 on Now Playing text block)
- Status bar 34px · screen title row ~57px (27px bold + back chevron)
- List rows: 54–66px tall, 1px `line` bottom border, 13px gap between
  art/text/meta. Hit target ≥44px everywhere.
- Buttons: 44–56px tall. Primary action = solid `acc` fill, `accInk`
  text. Secondary = 1px `line` border, `dim` text. NO border radius
  anywhere except: play button (circle), toggle knob (square), artist
  fallback (circle).
- Toggles: 40×22 outer (34×18 in headers), square 14px knob, border +
  knob = `acc` when on / `line`+`faint` when off.

## 2. Screen inventory (all in the mock)

| id | screen | notes |
|----|--------|-------|
| lock | Lock | clock 88px mono-light, track + thin progress, "tap twice to wake", side keys stay active |
| nowplaying | Now Playing | day: full-bleed 480×480 art, viz strip, title/artist + codec line, progress, transport, 5-icon toolbar. night: 92px art thumb + text, viz lower |
| upnext | Up Next | numbered queue, now-playing row highlighted `panel`, footer: clear / save-as-playlist |
| library | Library | tabs SONGS/ALBUMS/ARTISTS/PLAYLISTS; pinned scope-aware shuffle bar (see §3); albums grouped by artist w/ caps headers; artists show cover stacks |
| artist | Artist page | eyebrow ARTIST, name 28/800, stats mono, shuffle-artist bar, 3-up album grid, top songs |
| menu | Menu | 10 rows: icon + label + live value + chevron |
| eq | Equalizer | 5 preset chips + 10 draggable bands ±10 dB (32…16k), dashed 0 line, footer reset/save |
| sound | Sound Settings | DSEE HX, Vinyl Processor, VPT (Off/Studio/Club/Concert Hall), DC Phase (Off/Std A/B/Low A/B), Dynamic Normalizer, ClearAudio+ (bypasses others — show warning); live SIGNAL PATH readout in footer |
| bluetooth | Bluetooth | header toggle; connected card (codec + HP battery, Disconnect / Quality cycler); paired list = one-tap connect; big Pair-new button; NFC hint; receiver link |
| pairing | Pair new | NFC one-touch card on top, scanning state w/ progressive discovery, PAIR → PAIRING… → connected, tip footer |
| receiver | BT Receiver | toggle; discoverable-as-NW-A55 state; "EQ+DSP apply to received audio" |
| fm | FM Radio | 86px frequency, tuning ruler 76–108 MHz w/ accent needle, ±0.1 / seek buttons, 6 preset grid (hold to save), wired-headphones-as-antenna note |
| usbdac | USB-DAC | toggle; active: signal readout (input PCM, source, DSP, output); off: explainer |
| settings | Settings | DISPLAY (theme Day/Night, screen-off timer, brightness) · SYSTEM (storage, database rebuild, battery care, USB mode) · ABOUT |

Overlays: **Shelf** (bottom sheet: undo/redo history, pin-this-place,
3 pin slots) · **Volume** (top strip, auto-hides 1.4s, 0–120 range).

## 3. Behavior contracts

- **Navigation**: stack-based. Back chevron pops. Menu is the hub;
  Now Playing toolbar deep-links (queue/eq/bt/library). Persist the
  stack across sleep.
- **Shuffle semantics** (Library shuffle bar is one component, scope
  from active tab):
  - Songs → uniform shuffle over all tracks
  - Albums → shuffle album ORDER, tracks within album stay in sequence
  - Artists → random artist, shuffled within that artist
  - Per-row shuffle glyphs scope to that one album/artist.
- **Lock**: power key toggles. Locked = lock screen, touch rejected
  except double-tap wake; side keys still control playback/volume.
- **Bluetooth quick path**: paired-device rows are one-tap reconnect.
  Codec "Quality" button cycles LDAC → aptX HD → aptX → AAC → SBC.
  NFC touch pairs from any screen.
- **ClearAudio+** mutually exclusive with manual EQ/DSP — keep the
  settings but bypass them, show the warning line.
- **EQ**: 10 bands ±10 dB integer steps; editing any band switches
  preset label to custom slot A1/A2.
- **Status bar** (every screen): clock · codec badge (live) · NIGHT
  tag · shelf glyph · BT glyph (dim=connected/faint=not) · battery %.

## 4. Assets

- Icons: single-color strokes, ~1.7px stroke at 24px grid — recreate
  as vector paths or pre-rasterized glyphs per theme color.
  Set: play/pause/prev/next/shuffle/repeat/heart/queue/eq/bt/rx/usb/
  radio/settings/library/note/sound/chevrons/lock/bookmark/nfc/battery.
- Album art: decode embedded covers to 480×480 (day) + 92×92 thumb;
  cache a pre-darkened night variant or multiply at draw time.
- No other imagery. Visualizer = N vertical rects from FFT magnitudes
  (mock uses 36 bars, 3px gap; every 4th bar accent, rest `line`).
