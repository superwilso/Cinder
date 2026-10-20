# Localization

## English-first contributions

English is the default and remains sufficient for implementing and releasing a feature.
Contributors are not required to translate new UI copy, edit the Chinese catalog, or rebuild the
Chinese font subset when they add or change English UI text. An unknown label or formatted
template falls back to English in both languages. Prefer the existing UI translation helpers so a
translation can be added later without changing feature behavior.

Missing translations are not a CI failure and must not block a feature or release. The catalog
check verifies generated-file consistency and template arguments; font coverage verifies the
translations already in the catalog. Contributors who change the Chinese catalog maintain its
generated output and font coverage. Chinese translations may lag behind English functionality.
No ongoing translation work is required from the upstream maintainer.

## Translation source and runtime

Edit `player/cinder-ui/locales/zh-CN.json`; do not edit the generated
`player/cinder-ui/src/i18n_catalog.rs`. English source strings are the catalog keys. The runtime
embeds the generated Rust match table and does not read JSON or contact a translation service.
The generator also emits the `ui_format!` arms and rejects translated templates whose placeholder
names or order-independent placeholder sets differ from English.

Use `i18n::text("English UI label")` for fixed UI copy. Keep database IDs, action enums, setting
keys, paths, Sony return values, and user metadata in their stable domain representation. Translate
UI copy at the display boundary. Pass track, artist, album, playlist, and device names as unchanged
arguments to that copy; never translate those values themselves.

When one English label has different meanings in two controls, use
`i18n::text_with_key("SPACE", "Sound-space section")` at the call site that knows the meaning.
Keep the keyboard's spacebar on the ordinary `SPACE` key. A fixed bilingual chrome label may wrap
its own draw in `text::with_fixed_ui_chinese_glyphs`; keep that scope on the known UI label so
English metadata continues through the normal Sony font chain.

Dynamic text rendered immediately can use `ui_format!` with explicit arguments. A dynamic notice
that remains cached in `App` uses `ToastTemplate { key, args }`: the exact catalog key and owned
parameter values survive until render, when the current language formats them. Message identity is
never inferred from punctuation or a previously formatted sentence. This distinguishes, for
example, adding a playlist named `Up Next — Blue` from adding a track named `Blue` to Up Next even
though the English sentences are identical. Parameter contents are appended verbatim, including
braces, separators, translated-looking words, or punctuation. External error text should be passed
as raw text and must not be looked up as a UI message.

Each `App` stores `Language::English` or `Language::SimplifiedChinese`. Persist only `en` and
`zh-CN`; unknown and missing tokens fall back to English. `i18n::enter` scopes the active language
to the rendering thread and restores the previous value on drop, including nested renders and early
returns. Changing language does not rewrite domain state or clear a cached notice.

## Generate and check

From the repository root:

```sh
python tools/generate_locales.py
python tools/generate_locales.py --check
cargo fmt --manifest-path player/Cargo.toml --all -- --check
cargo test --manifest-path player/Cargo.toml --release
```

The first command is deterministic and writes the catalog. The second is the same check wired into
the player CI job. The Rust test suite checks placeholder-driven UI behavior and that the bundled
Chinese subset covers every CJK codepoint in the translation source and language picker. If a new
translation introduces a glyph absent from the subset, that test fails with the missing character.
It also checks the explicit language-token fallback and render-context isolation.

To rebuild the UI font subset, install the pinned dependency and provide the verified upstream
source font:

```sh
python -m pip install fonttools==4.63.0
python tools/subset_ui_font.py /path/to/NotoSansCJKsc-Regular.otf
```

The source is [Noto Sans CJK SC Regular](https://github.com/notofonts/noto-cjk/blob/main/Sans/OTF/SimplifiedChinese/NotoSansCJKsc-Regular.otf),
SHA-256 `2c76254f6fc379fddfce0a7e84fb5385bb135d3e399294f6eeb6680d0365b74b`. The generator refuses a
source with a different hash. The generated subset is distributed under SIL Open Font License
1.1; the checked-in subset is 118,128 bytes with SHA-256
`d3fe2e13b678114dca21c5ed70d51179c943154ea6af5b18635b354048afd814`. Retain
`player/cinder-ui/assets/fonts/CinderHansUI-OFL.txt` with it. The checked-in Hanken
Grotesk and JetBrains Mono fonts retain their own OFL notices.

The subset contains only the UI catalog and picker glyphs. It is selected during Simplified Chinese
UI rendering and inside an explicit scope for a fixed bilingual UI label such as `Language / 语言`.
English mode keeps using the existing device fallback chain for Chinese metadata; its typography
therefore depends on the Sony font files installed on that player. The subset is not an unconditional
fallback for English metadata. Font loading remains lazy, cached, and bounded; the implementation
does not load several large system fonts to search for one glyph.

## Preview and verification boundary

`cargo run --manifest-path player/Cargo.toml --release -p cinder-ui --example language_smoke`
exercises an immediate Settings language switch, English round-trip, and FM actions in both
languages. The full host preview set can be rendered in each language with
`cargo run --manifest-path /path/to/cinder-i18n-pr/player/Cargo.toml --release -p cinder-host -- --language en`
and the same command ending in `--language zh-CN`. The host renderer writes `out/` under its current
working directory; use separate isolated working directories to keep both sets. The host previews
and UI tests can check layout and catalog coverage, but they do not prove the
player's Sony font fallback, ARM ABI, device-only screens, audio behavior, or readability on the
physical panel. Inspect Settings, Sound, Advanced, menus, dynamic notices, confirmation dialogs,
and long track information at 100% and 140% before accepting a UI change. English previews should
also be compared with the upstream rendering, especially for Chinese metadata.
