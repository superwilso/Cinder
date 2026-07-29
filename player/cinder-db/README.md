# cinder-db — read-only Sony media-library reader (SQLite)

Pure-Rust reader for the device library DB (**SQLite at `/db/MTPDB.dat`** — Wampy's path,
confirm on device). No libc++, no IPC — the whole library/metadata/now-playing-text layer is
Rust. Schema reverse-engineered from `libMediaStoreService.so`
(see `../../analysis/H_mediastore/RE_findings.md`).

## API (`Db`)
- `Db::open(path)` — read-only (`SQLITE_OPEN_READ_ONLY`, won't disturb the scanner).
- `albums()` → `Vec<Album{id,name,track_count}>`
- `artists()` → `Vec<Artist{id,name}>`
- `album_tracks(album_id)` / `tracks(Sort::{Title|Artist|Length|Added})` → `Vec<Track>`
- `track_by_filename(uri)` → `Option<Track>` — resolve now-playing metadata from PlayStatus.uri
- `art_for_object(object_id)` → `Option<Art>` — album art via `othumb_id → images`

`Track` = object_id, title, artist, album, filename, disc_no, track_no, `duration_raw`
(DURATION ext-int prop; units = the DB's — calibrate on device, likely ms), is_hires, othumb_id.

## Schema notes (RE'd)
- `object_body` = items/tracks; track rows have a non-null `filename`. FKs → `albums`/`artists`
  (`value` = name).
- **Duration / codec / samplerate etc. are NOT columns** — they're in `object_ext_int`
  (object_id, akey, value), keyed by an `akey` whose name is in the `schema` table
  (`prop_name='DURATION'`). The reader resolves that akey once and inlines it.
- Album art: `object_body.othumb_id`/`mthumb_id → images.id`; `images` has `bmpfile`
  (pre-rendered) or `(value path, dataoffset, datasize)` for an embedded blob.

## Build
Host: `cargo test -p cinder-db` (fixture DB with the real schema — 6 tests, no device needed).

Cross (device glibc) — bundled SQLite compiles C, so point the `cc` crate at the cross gcc:
```bash
CC_arm_unknown_linux_gnueabihf=arm-linux-gnueabihf-gcc \
AR_arm_unknown_linux_gnueabihf=arm-linux-gnueabihf-ar \
cargo build -p cinder-db --release --target arm-unknown-linux-gnueabihf
```
(The same env is needed when `cinder-ffi` — which will depend on this — is cross-built.)

## Open items (calibrate against a real MTPDB.dat)
- Confirm `/db/MTPDB.dat` path + that it's plain SQLite (`file` / `.tables`).
- `DURATION` units (ms vs s); `media_type`/`object_type` values if we want stricter track filtering.
- Album-art blob: confirm `bmpfile` exists on device vs. needing the `(path,offset,size)` blob;
  whether embedded cover art is also extractable.
- Text encoding of `title`/`value` (expect UTF-8).
