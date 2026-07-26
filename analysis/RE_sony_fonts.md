# Sony's on-device font set (NW-A55 / NW-A50, stock v1.02)

Reverse-engineered 2026-07-26 from the extracted stock rootfs
(`analysis/binwalk/6.bin/_6.bin.extracted/ext-root`). Relevant because **Cinder's bundled fonts
are Latin-only**, so every non-Latin tag in a user's library rendered as `.notdef` boxes until
Cinder started borrowing these at runtime (`player/cinder-ui/src/text.rs`, `FALLBACK_FONTS`).

## Location and inventory

`/system/vendor/sony/lib/fonts/` — 18 files, ~41 MB total. Fontconfig lives at `/system/etc/fonts/`
(`fonts.conf`, `local.conf`, plus a prebuilt `*.cache-4`), i.e. Sony's Qt UI resolves these through
ordinary fontconfig.

| File | Size | Script coverage |
|---|---:|---|
| `SST-{Roman,Light,Bold}.otf` | ~88 KB ea | Latin + Latin-ext + **Cyrillic + Greek**, proportional |
| `SSTUI-{Roman,Light,Bold}.ttf` | ~490 KB ea | Same, hinted UI cut (see the parse bug below) |
| `SSTJpPro-{Regular,Light,Bold}.otf` | ~2.9 MB ea | **Japanese** kana+kanji, Hans, Cyrillic, Greek, `♪ ♥` |
| `NotoSansKR-{Regular,Light,Bold}.otf` | ~4.6 MB ea | **Hangul** (+ JP/Hans) |
| `DFPGothicPW5-BIG5HK-SONY-20140613.ttf` | 10.1 MB | **Traditional Chinese** (BIG5-HK) |
| `DFHeiW5-A-SONY-CTC.ttf` | 7.9 MB | Traditional Chinese (CTC) |
| `DFGothicPW5-BIG5HK-SONY-20131004part.ttf` | 1.4 MB | Partial Trad. Chinese |
| `NotoSansThai-{Regular,Bold}.ttf` | ~30 KB ea | **Thai only** — no Latin at all |

**SST is Sony's corporate typeface** (Linotype, © 2012). Using it for scripts the Cinder face lacks
also happens to move those strings *closer* to stock's look, not further away.

`NotoSansThai` covers 2/15 ASCII — it is a pure fallback leaf and must never be earlier in a chain
than a Latin-capable face.

## The `SSTUI-Roman.ttf` parse bug (real, and specific to that one file)

`fontdue` 0.9.3 **rejects `SSTUI-Roman.ttf`**:

```
Attempted to map a codepoint out of bounds.
```

`SSTUI-Light.ttf` and `SSTUI-Bold.ttf` from the same directory parse fine, as does every other font
in the set. The three are structurally near-identical (same 21 tables, `indexToLocFormat=1`,
`numGlyphs` 617/616/616), and **all cmap subtables are clean** — format 4 (0,3) and (3,1) plus a
format 6 Macintosh table, zero out-of-range entries in any of them.

The fault is in **GSUB**. `fontdue` loads substitution glyphs by default
(`FontSettings::load_substitutions`) and then hard-errors on any index `>= numGlyphs`
(`fontdue-0.9.3/src/font.rs:288`). Walking the 21 lookups finds two bad single-substitution entries:

| Lookup | Format | Substitution |
|---|---|---|
| type 1 | fmt 2 | glyph `615` → `65535` |
| type 1 | fmt 1 | glyph `615` → `65535` |

`65535` (`0xFFFF`) is out of range for `numGlyphs = 617`. `SSTUI-Bold.ttf` has 20 lookups and zero
out-of-range substitutions — so this is an authoring defect in the Roman cut specifically, not a
family-wide trait.

FreeType/HarfBuzz (what Sony's Qt stack uses) silently ignores an invalid substitute, which is why
the font works perfectly well on the stock UI. Strict parsers do not.

**Two workarounds, if this font is ever wanted:**
1. `FontSettings { load_substitutions: false, ..Default::default() }` — parses cleanly, and glyph
   lookup is unaffected (`'Ч'` → glyph 447). Cinder does no shaping, so it loses nothing.
2. Use `SSTUI-Bold.ttf` / `SSTUI-Light.ttf`, or `SST-Roman.otf` (10× smaller and unaffected).

Cinder takes route 2: `SST-Roman.otf` is in the fallback chain and `SSTUI-Roman.ttf` is explicitly
excluded.

## Why the chain is ordered the way it is

`SSTJpPro` covers Cyrillic and Greek too — but as **full-width** glyphs (16 px advance at 16 px vs
~9.6 proportional). A Russian album title resolved to it renders `Ч а й к о в с к и й`, visibly
spaced out and prone to truncation. `SST-Roman.otf` is proportional, 87 KB, and correct for those
scripts, so it goes first and the big CJK faces only ever serve scripts they are actually for:

```
SST-Roman.otf  →  SSTJpPro-Regular.otf  →  NotoSansKR-Regular.otf  →  DFPGothicPW5  →  NotoSansThai
   Cyr/Greek         JP + Hans + ♪♥            Hangul                  Trad. Chinese       Thai
```

Each is loaded **only when a glyph actually misses**, so a Latin-only library pays nothing. Nothing
is redistributed — the files are read at runtime from the device's own `/system`, and on a host the
paths simply don't exist, which makes the chain an inert no-op.

Verified by `player/cinder-ui/tests/font_coverage.rs` and visible in the host harness renders
`i18n_library_songs` / `i18n_now_playing` (`cargo run -p cinder-host`, with `CINDER_FONT_DIR`
pointed at the extracted rootfs).
