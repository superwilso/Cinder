# Translating Cinder

Cinder's menus and messages can be shown in more than one language. English is the source, and
every other language is a translation of it. The installer has one checkbox per language: whoever
installs picks the languages they want, and if they pick more than one, Settings › Language
switches between them on the player.

You do not need to know Rust to translate. You need Python 3 and a text editor. A Rust toolchain
lets you check your work on screen before you send it, but the pull request checks will do that
for you if you don't have one.

## Add a language

The example adds German (`de`). Use the [BCP 47 tag](https://www.w3.org/International/questions/qa-choosing-language-tags)
for your language: `de`, `fr`, `pt-BR`, `zh-TW`.

1. **Say you are starting.** Open an issue with the *Translation* template, so two people don't
   translate the same language at once.
2. **Make the file.** From the repository root:

   ```sh
   python3 tools/generate_locales.py --new de
   ```

   This writes `player/cinder-ui/locales/de.json` with every English string Cinder shows, each
   with a blank translation.
3. **Register the language.** Add an entry to `player/cinder-ui/locales/languages.json`:

   ```json
   {
     "tag": "de",
     "id": "German",
     "name": "Deutsch",
     "english": "German",
     "word": "Sprache"
   }
   ```

   `name` is the language's name in itself. `word` is your word for "language": the Settings row
   is titled with that word in every installed language, such as `Language / Sprache`, so a reader
   stuck in a language they can't read can still find it. `id` is the English name as one
   CamelCase word. The order of the list is the order the languages are offered in.
4. **Add the installer checkbox.** In `cinder-home/deploy/components.conf`, copy the
   `lang-zh-cn` entry and change it:

   ```
   lang-de | CINDER_LANG_DE | bool | 0 | Language: Deutsch (German)
       The player's menus and messages in German. The names of your songs, artists and
       albums stay as they are.
   ```

   The variable is `CINDER_LANG_` plus your tag in upper case, with `-` written as `_`
   (`pt-BR` becomes `CINDER_LANG_PT_BR`). Nothing else in the installer needs to change.
5. **Translate.** Fill in the blanks in `de.json`. The rules are below. You don't have to finish:
   a blank string shows in English, so a partial translation is still useful.
6. **Generate and check:**

   ```sh
   python3 tools/generate_locales.py
   python3 tools/generate_locales.py --status de
   ```

   The first command builds the code the player uses from your file. The second shows how much is
   done and lists every string still blank.
7. **Open a pull request** with the two JSON files, `components.conf` and the generated
   `player/cinder-ui/src/i18n_catalog.rs`.

## Rules for translating

- **The key is the English, the value is yours.** Change only the right-hand side:
  `"Sleep timer": "Schlaftimer"`. Never edit a key: the player finds a translation by it.
- **Keep every `{…}` exactly as it is.** `"Added to {name}"` must keep `{name}`, and
  `"Slot {} cleared"` its `{}`. Move them wherever your grammar needs them. The generator stops
  with an error if a translation's placeholders differ from the English.
- **Leave `%s` alone too.** A few messages come from Cinder's C++ side and use `%s` the same way.
- **Copy the case.** English labels in capitals (`SHOW`, `SAFE LEVEL`) are drawn small, and the
  capitals keep them readable. Use capitals where your script has them. Where it doesn't, write
  the label normally.
- **Keep it short.** The screen is 480 pixels wide. If your translation is much longer than the
  English, the test suite will tell you where it runs out of room (`tests/ui_overflow.rs`).
- **Don't translate names.** Leave Sony, LDAC, ClearAudio+, DSEE, Walkman One, Flint and Wampy as
  they are. Your users' own song, artist and album names are never translated; the player keeps
  them apart from the UI text.
- **One English word can have two meanings.** Where it does, the code gives one of them its own
  key, which describes the meaning (`"Sound-space section"` for the `SPACE` heading, as opposed to
  the space bar). Translate what the key describes.

## Fonts

The player can draw a translation only from the fonts Cinder ships. A character those fonts don't
have makes it fall back to Sony's own fonts, which costs tens of megabytes and can run the player
out of memory. So the test suite draws every character of every translation, and it fails with
the file and character if one is missing:

```
locales/de.json: 'Ж' (U+0416) is not in Cinder's bundled fonts; docs/LOCALIZATION.md, "Fonts"
```

- **Latin-script languages** (German, French, Spanish, Portuguese, Polish…) draw from Cinder's own
  fonts. If the check names a character, use a common alternative where your language has one, or
  say so in your issue.
- **Chinese, and other languages written with CJK characters**, draw from a small subset of Noto
  Sans CJK that holds only the characters the translations use. Set `"cjk_font": true` in your
  `languages.json` entry and rebuild the subset (below). Japanese needs Japanese glyph shapes, so
  it will need its own subset: ask in an issue first.
- **Any other script** (Cyrillic, Arabic, Hebrew, Thai, Devanagari…) needs a font added to
  Cinder first, and right-to-left scripts need layout work too. Cinder's fonts do have Greek, so
  try a Greek translation against the check before asking. Open an issue before you start, so
  your work is not stuck waiting on it.

To rebuild the CJK subset after adding characters, install the pinned dependency and download the
source font:

```sh
python3 -m pip install fonttools==4.63.0
python3 tools/subset_ui_font.py /path/to/NotoSansCJKsc-Regular.otf
```

The source is [Noto Sans CJK SC Regular](https://github.com/notofonts/noto-cjk/blob/main/Sans/OTF/SimplifiedChinese/NotoSansCJKsc-Regular.otf),
SHA-256 `2c76254f6fc379fddfce0a7e84fb5385bb135d3e399294f6eeb6680d0365b74b`. The tool refuses any
other file, and the same source and tool always give the same subset, so a reviewer can rebuild it
and compare. The subset is under the SIL Open Font License 1.1; keep
`player/cinder-ui/assets/fonts/CinderHansUI-OFL.txt` with it.

## See it on screen

With a Rust toolchain, render every screen in your language:

```sh
mkdir -p /tmp/cinder-de && cd /tmp/cinder-de
cargo run --manifest-path /path/to/Cinder/player/Cargo.toml --release -p cinder-host -- --language de
```

The pictures land in `out/` in the folder you run it from. Before you send the pull request, look
at Settings, Sound, Advanced, the menus, a few messages and a confirmation dialog. Then run the
tests the pull request runs:

```sh
python3 tools/generate_locales.py --check
cargo test --manifest-path player/Cargo.toml --release
```

## Keep a translation up to date

New features arrive in English first. Missing translations never fail a build or hold up a release:
the new strings simply show in English. To see what your language is missing:

```sh
python3 tools/generate_locales.py --status de
```

The list holds every string a locale file already has a key for, plus the strings the code passes
straight to a translation call. An English string you see on the player but not in the list
reaches the screen through a table or a constant. Add it to your file by hand, with the English
exactly as it appears, and it will be picked up.

## For developers

- **Mark UI text where it is drawn.** Use `i18n::text("English label")` for fixed text and
  `ui_format!("{n} tracks", n = count)` for text with values. Keep database values, actions,
  settings keys, paths and Sony's return values in English, and translate only at the display
  boundary.
- **Never translate user data.** Pass track, artist, album, playlist and device names as arguments
  to the UI text, and never through `i18n::text` themselves.
- **Messages that wait to be shown.** A message kept in `App` until it is drawn uses
  `notify_format(key, args)`, so it is formatted in the language in use when it appears. External
  error text goes through `notify_raw` and is never looked up.
- **Two meanings, one English word.** Use `i18n::text_with_key("SPACE", "Sound-space section")`
  where the code knows which one it means.
- **Language state.** Each `App` has its language and the list the installer offered
  (`set_languages`, read from `/data/cinder/languages` by `cinder-ffi`). The settings file stores
  the tag (`language=zh-CN`), and a tag that isn't installed is ignored. `i18n::enter` sets the
  language for one render on the current thread and puts the previous one back when it drops.
- **Generated code.** `i18n_catalog.rs` is generated from `locales/` and must not be edited. It
  holds the `Language` enum, the lookup tables and the `ui_format!` arms; each arm is its own
  `format!`, so Rust checks the arguments for every language. CI runs
  `generate_locales.py --check`, which also fails when a language has no installer checkbox.
- **On the device.** `install_cinderhome.sh` writes every ticked `CINDER_LANG_*` tag to
  `/data/cinder/languages`, one per line, without knowing the list. With no file, or none of the
  tags known, the player is English only.
