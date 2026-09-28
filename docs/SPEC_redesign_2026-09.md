# The 2026-09 redesign — the spec

This is the handoff's written spec, kept in the repository so the code can point at it. It
covers the Cinder shell, [Flint](https://github.com/superwilso/flint)'s window and the Windows
installer. The ledger is [`PLAN_redesign_2026-09.md`](PLAN_redesign_2026-09.md). It says which of
these screens exist, which are planned, and where the build departs from the drawings on purpose.

**Fidelity:** high. Colours, sizes and copy are final unless marked *placeholder*. Every count,
size and timing drawn in Flint and SensMe is a placeholder.

**Where the numbers live in code:**
* the shared anatomy is `player/cinder-ui/src/kit.rs`;
* the tokens are `player/cinder-ui/src/theme.rs`;
* the type scale is `player/cinder-ui/src/scale.rs`.

A number in this document and a constant in those files must agree. If they disagree, the code is
the thing that ships, and this document gets corrected.

---

## 1. What the design must not break

These come from the design brief. They outrank anything drawn.

* **Never brick.** Every change keeps a path back to stock, and an escape depends on less than the
  thing it rescues.
* **No new literal colours.** Every colour comes from a `Theme` token. The one colour the mocks use
  that `theme.rs` did not have — the control border, `#3a352e` against the default palette — is
  derived: `Theme::ctrl()` sits two fifths of the way from `line` to `faint`, so every palette,
  accent and night level gets it for free.
* **Legible everywhere.** Any new surface stays readable under all six accents, day and night, at
  the five night levels and at every UI scale.
* **Flat.** No motion, no gradients, no blur. The renderer is a dirty-flag software raster on a
  Cortex-A7.
* **Unbounded text is fitted, never overflowed.** It goes through `widgets::fit` or `draw_fit`; the
  Now Playing title marquees.
* **Touch only, plus hardware buttons.** The player has no d-pad. The minimum hit target is 44 px.
* **Text never sizes a target.** Hit boxes are pure functions of layout (chips are equal width), so
  the UI-scale slider cannot move what a finger lands on.

---

## 2. Cinder (480 × 800)

### 2.1 Shared anatomy (every list screen)

| Part | Spec | In code |
|---|---|---|
| Status bar | 44 px. Clock at x18, mono 13 `dim`. Format badge at x72: 21 px high, 1 px `acc` border, mono 12, tracking .12em, `acc`. Battery % right-aligned to x448, then an 18 × 13 outline | `chrome.rs` (unchanged) |
| Header | 47 px. Back chevron at x20, only on pushed screens. Title Sans Bold 30, tracking −.01em, `ink`. **The right slot is exactly one of:** nothing · a caption (mono 13, tracking .14em, `faint`, or `acc` when it is an action: DONE / SAVE / NEW / SHUFFLE) · a switch · an icon | `chrome::header` |
| Strip (optional) | 40 px, `panel`, 1 px `line` above and below. Mono 13, tracking .06em, `dim`. One line of state: what is playing, a count, a path | `kit::strip`, `STRIP_H` |
| Section label | 34 px, bottom-aligned with 6 px under the text. Mono 13, tracking .16em, `faint`. Optional action on the right in `acc` | `kit::section_label`, `SECTION_H` |
| Row | 64 px, 1 px `line` under it, padding 0 20 px, gap 14. Title Sans SemiBold 19 `ink`; subtitle Sans 16 `dim`, 2 px below. Optional lead: 26 px, mono 14. Optional value: mono 13, tracking .1em. Optional chevron: 18 px `faint`. Selected row: `row_sel` background, title in `acc` | `kit::row`, `ROW_H` = `scale::SETTING_ROW_H` |
| Switch | 40 × 22, 1 px border. On: `acc` fill, a 16 px knob in `acc_ink` at x20. Off: `ctrl` border, `dim` knob at x2 | `kit::switch` |
| Chips | 44 px tall, padding 0 16 px, gap 8, Sans SemiBold 16. Selected: `acc` fill and `acc_ink` text. Otherwise a 1 px `ctrl` border | `kit::chips`, `kit::chip_at` |
| Primary button | Full width less 40, 56 px, `acc` fill, Sans Bold 19 in `acc_ink`. **At most one per screen** | `kit::primary_button` |

### 2.2 Tokens (`theme.rs`, the default palette, day)

| Token | Value |
|---|---|
| `bg` | `#0d0c0b` |
| `panel` | `#13110f` |
| `line` | `#221f1b` |
| `ink` | `#ece7df` |
| `dim` | `#95908a` |
| `faint` | `#5f5a52` |
| `acc` (amber) | `#f4651f` |
| `acc_ink` | `#1a0a02` |
| `row_sel` | `#1c1713` |
| control border | `#3a352e` in the mocks; `Theme::ctrl()` in code (§1) |

Night values and the other five accents are unchanged from `theme.rs`. The mocks use Hanken
Grotesk for `Family::Sans` and JetBrains Mono for `Family::Mono`, the two families Cinder ships.

### 2.3 Screens

| ID | Screen | File | What changes |
|---|---|---|---|
| 1a | Now Playing, Library, Queue, Search | `now_playing.rs`, `library.rs`, `up_next.rs`, `search.rs` | A view bar: sort · filter · rows. Swipe a row left for play next and right for play later, the same in every list. A sideways swipe on a page changes page. Back is the bar arrow only |
| 2a / 2b | Sound, Equalizer | `sound.rs`, `eq.rs` | A signal-path strip under the header. A / B profiles per output |
| 2c | Settings | `settings.rs` | The Display row's subtitle: "Palette · accent · volume display" |
| 2d | Lock | `lock.rs` | **No layout change.** The mock is `lock.rs` as it is |
| 2e | Volume HUD | `overlay.rs` | **Full** is the existing pill (x24, y = `STATUS_H` + 12, 432 × 40). **New: Minimal** — a 3 px bar across the whole width at y = `STATUS_H` (track `line`, fill `acc`), with no icon or number, and the same `VOL_FRAMES` timeout. Chosen in 5k |
| 2f | FM radio | `fm.rs` | The meter and fast scan appear only with the FM helper |
| 2g | Bluetooth | `bluetooth.rs` | The connected card keeps the codec line, the name and DISCONNECT. The kbps line and the LDAC quality list move to 5l. A new section, **THIS DEVICE**, has two rows: Sound quality (value `990`, chevron → 5l) and Sound profile (value `B`) |
| 2h | USB-DAC | `usbdac.rs` | The comparison with stock is removed |
| 5a | Menu | `menu.rs` | "Start on" chips: Library · Now Playing · Menu · Last screen. A HOME tag on the chosen row. The strip shows what is playing |
| 5b | Playlist, editing | `playlist_pick.rs` | ≡ handle to move, × to remove, UNDO in the section label, DONE in the right slot. Saved as `.m3u8`, so Flint keeps the edit |
| 5c | Saved view | `library.rs` | Rules (rating, last played, format, + add), sort chips, "show as" chips, and a "Pin to the Library bar" switch |
| 5d | SensMe | `sensme.rs` | A 2-column grid of the 12 channels, each tile 68 px with a 4 px count bar. A "Follow the time of day" switch. A Play button |
| 5e | Battery | `device.rs` | A big % (mono light 44) with a 10 px bar. The CHARGER section appears only with the battery helper; without it, one line: "Needs the battery helper." |
| 5g | Playlists | the Library's Playlists tab | SMART (saved views, with a ◇ lead in `acc`) above YOURS. An EDITED tag on playlists changed on the player. (The handoff names `shelf.rs`; the Shelf does not change — see the ledger, Part G) |
| 5h / 5m | Album, Artist | `library.rs` | Album: the rating in the right slot, the track rows, and the playing row in `row_sel`. Artist: albums newest first with their ratings, then the 3 most played |
| 5i | Help & controls | `onboarding.rs` | The way back (3 rows), gestures, buttons. Values only, no explanations |
| 5j | Palette | `palette.rs` | Lists what `load_files` returned: the built-in palette first, then by name or by date added (sort chips). Each row has a 4–5 colour swatch (18 × 28 cells), and the selected row uses `row_sel`. A **SKIPPED** section has one row per refused file, with the first `problems()` sentence as its subtitle. An **ADD** row: "Copy .palette files to cinder_palettes/" |
| 5k | Display | `display.rs` | Palette (the value and a swatch) → 5j. Accent ("—" and "Set by the palette" when the palette pins its own accent). A Night switch. **Volume: Full / Minimal** chips. Size. Visualiser |
| 5l | Bluetooth ▸ Sound quality | `bluetooth.rs` | LDAC: 990 / 660 / 330 / AUTO, the selected one in `acc`. VOLUME CONTROL: Fine volume (±2 dB), Steps (127) |

### 2.4 New settings keys

`volume_hud = full|minimal`, `home_screen = library|now_playing|menu|last`, `palette_sort =
name|added`. The build adds one more, `home_last`: the place Last screen reopens, written only when
it is chosen.

---

## 3. Flint (window 920 × 760)

* **Band:** 64 px `#1a1818`, with a 3 px `#e0551b` rule at y61. "Flint" in Segoe 600 20, `#f2efec`.
  The device line is Segoe 12 in `#8e8884`.
* **Tabs.** They are right-aligned, and in the same order in every file: Sync · On the player ·
  Check · SensMe · Likes & plays · Palettes · Settings. Each is 34 px with radius 5. The active tab
  is a `#f2f3f6` background with `#191a1e` text; inactive text is `#cfcbc7`. Counts are Segoe 12.
* **Sync sub-row:** Plan · Conversion · Copying. 32 px, Segoe 600 13, with a 2 px `#e0551b`
  underline on the active one.
* **Light tokens:**

  | Token | Value |
  |---|---|
  | window | `#f2f3f6` |
  | card | `#fff`, with a 1 px `#e3e4ea` border and radius 8 |
  | row divider | `#eeeff3` |
  | ink | `#191a1e` |
  | secondary | `#5f6069` |
  | control border | `#d3d4dc` |
  | accent | `#e0551b` |
  | warn | `#b32e14` |
  | ok | `#2e7d4f` |

* **Dark tokens:** the accent is unchanged.

  | Token | Value |
  |---|---|
  | window | `#141416` |
  | card | `#1d1d21` |
  | border | `#2c2c32` |
  | divider | `#26262b` |
  | ink | `#ecebe8` |
  | secondary | `#9c9ca6` |
  | control | `#3a3a41` |
  | warn | `#ff8a6a` |
  | ok | `#6cc795` |

* **Appearance:** Settings ▸ Theme: Light / Dark / System, default System.

| Page | Notes |
|---|---|
| Setup | First run only, with no tabs. Library, player and likes cards; facts as 24 px chips |
| Sync ▸ Plan | Capacity per card, and the plan as tickable rows. "Copy to the player" is the only orange control |
| Sync ▸ Conversion | A format × card grid. Clicking a cell cycles keep → FLAC 16/44 → AAC 256. Totals update live |
| Sync ▸ Copying | The plan rows, each with a state (✓ › · !). One bad file skips one track and shows one line |
| On the player, Check, Likes & plays | As drawn in round one |
| SensMe | Progress, 12 channel bars, and NOT TAGGED counts by reason |
| Palettes | A table of name · swatch · added · player state (BUILT IN / ON / NEW / REFUSED), sortable by Name or Added. A preview panel renders the selected palette. The same readability rules as `palette.rs::problems()` run before sending. "Send N to the player" copies the ticked files to `cinder_palettes\` on the player |
| Settings | Appearance, the library folder, the conversion cache (Clear), Last.fm, what likes become, and the **Always keep** album list (never converted) |

---

## 4. Installer (window 920 × 720, `installer/src/gui.rs`)

The palette is unchanged:
* band `#1a1818` with a 2 px `#e0551b` rule;
* page `#f6f6f8`;
* accent `#e0551b`;
* warn `#b32e14`.

Step pills in the band: Player · Options · Confirm · Done.

| Page | Spec |
|---|---|
| Player | A drive picker and Rescan. A 4-cell fact strip: PLAYER · FIRMWARE · CINDER · LAST BOOT (from the launcher breadcrumb). An alert card for a CFW folder or a hand-back to stock, with the evidence line in mono. Three 64 px action rows; the one the state implies is filled orange |
| Options | Three groups (Features / Hardware access / Sound tuning) in 44 px rows with cost tags: `setuid root` red, `needs Wampy` / `needs tags` amber, `new` blue, `dev only` grey. Enum options are inline segmented controls. A 300 px detail pane: what it does, what it costs, and what you lose without it. It checks the drive for the volume-table file on the spot. A • marks parts that differ from the player |
| Confirm | A diff table (↑ version, ~ changed, + / − for install / uninstall), a Kept / Needs block, and a 4-step timeline |
| Installing | An 8 px progress bar, 6 named steps (from `stage.rs`) and the log panel. "Don't unplug the player." |
| Done | What to expect on the player (3 lines), the recovery ladder from `RECOVERY.md`, "Remove staged files" (`--clean`) and Close |
| Failed | Where it stopped (the step list, with `!` on the failed step), the recovery ladder, "Open the install log" and "Rescan for the player" |

Home-page scenarios to handle:
* installed;
* fresh;
* Walkman One detected;
* handed back to stock.

**Not in the handoff:** the stock-like shell variants, which are on hold.
