# MediaStore RE — library browse + metadata + album art

How Cinder gets real library data (albums/artists/tracks/playlists), the now-playing
**title/artist/album/art** (PlayerService only gives a URI), and builds queues. **Major
finding: the store is a SQLite DB with a fully-visible schema** — so there are two paths, and
the simpler one (read SQLite directly) needs *no* C++/IPC shim at all.

## Path A — read the SQLite DB directly (recommended for browse + metadata)
The server (`libMediaStoreService.so`) creates a SQLite DB (Wampy reads it at **`/db/MTPDB.dat`**
— confirm path on device). Schema recovered verbatim from the binary:

- **`object_body`** (the items / tracks): `object_id` PK, `object_type`, `media_type`, `format`,
  `title`, `filename`, `filesize`, `disc_no`, `series_no`, `is_high_resolution`, `mtime`,
  `addedtime`, `datetime`, `published_date`, `rating_id`, and FKs `album_id`, `artist_id`,
  `albumartist_id`, `composer_id`, `genre_id`, `releaseyear_id`, `othumb_id`, `mthumb_id`
  (original/medium thumbnails), `sort_str`, `search_str`, `initial`, `parent_id`, `child_count`…
- **`albums`** / **`artists`** / **`albumartists`** / **`genres`** / **`videogenres`**:
  `id` PK, `value` (the name, UNIQUE), `sort_str`, `search_str`, `initial`. **`artists`** also has
  `imagefile`, `face_x/y/w/h` (artist art + face crop).
- Indexes on `album_id`, `artist_id`, `albumartist_id`, `genre_id`, `(initial,sort_str,title)`.

So the library screens are plain SQL:
- Albums list: `SELECT id,value FROM albums ORDER BY sort_str`. Track count per album:
  `SELECT album_id,COUNT(*) FROM object_body WHERE media_type=<audio> GROUP BY album_id`.
- Album's tracks: `SELECT object_id,title,filename,disc_no,artist_id FROM object_body
  WHERE album_id=? ORDER BY disc_no,child_index`.
- Now-playing metadata from a URI/filename: `SELECT ob.title, ar.value artist, al.value album,
  ob.othumb_id, ob.is_high_resolution FROM object_body ob LEFT JOIN albums al ON al.id=ob.album_id
  LEFT JOIN artists ar ON ar.id=ob.artist_id WHERE ob.filename=?`.
- Sort-by (the UI feature) maps to `ORDER BY` on `sort_str`/`title`/duration/`addedtime`.

Implication: **the library lives entirely in the Rust UI layer** — add `rusqlite` (bundled
SQLite, statically) to `cinder-ffi` and read `MTPDB.dat`. No libc++, no IPC, no service
dependency, matches Wampy's proven approach, and it's fast + read-only (safe).

**Album art:** `othumb_id`/`mthumb_id` → thumbnail objects; the IPC `Properties::GetPropertyReader`
→ `RawPropertyReader(path, offset, length)` shows art is a **blob at a (file, offset, len)** —
likely a packed thumb cache. Resolve the thumb path/offset (a thumbs table or a cache dir) on
device; fall back to embedded-cover extraction or the gradient placeholder Cinder already draws.

## Path B — drive MediaStoreClient (the official IPC API; for parity/scan/write)
Exported C++ (link `libMediaStoreServiceClient.so`), same clean style as PlayerService:
- `MediaStoreService::GetInstance()` → `MediaStoreClient::Connect()`.
- `GetCount(result_type_t[, IFilter&], uint&)` — counts per entity.
- `Search(result_type_t[, IFilter& | filter_type_t+string][, offset, limit], IIdList&)` — returns
  an `IIdList` of ids (with pagination + filtering).
- `CreateFilter()` → `IFilter` for compound queries.
- `GetProperties(uint id, uint64 mask, IProperties&)` / `GetPropertiesMap(vector<id>, mask, map)` —
  batch metadata fetch. `Properties::GetPropertyStr/U16str/Int32/Int64/Uint32(property_type_t,&)`
  typed getters; `GetPropertyReader(property_type_t, IPropertyReader&)` for art/blobs;
  `GetPropertySyncLyric`/`GetPropertyUnsyncLyric` for lyrics.
- Enums: `result_type_t` (album/artist/track/genre/playlist…), `filter_type_t`, `property_type_t`,
  `entry_type_t`, `language_t` — `MediaStoreServiceUtil::Convert*` map the I↔pst variants
  (decompile those for exact values *if* we go this route).

Use Path B only if we need the scanner's encoding handling, write/playlist edits, or to stay
perfectly in sync with on-device DB mutations. For read-only browse + now-playing, Path A wins.

## Recommendation / how it ties together
- **Library + now-playing text + sort/shuffle source → Path A** (rusqlite in `cinder-ffi`, read
  `/db/MTPDB.dat`). Library screens, real now-playing title/artist/album, and the data to build
  queues all come from SQL.
- **Playback control → PlayerService C++ shim** (`cinder-audio`, prior RE). Flow:
  `PlayStatus.uri → SQLite row → title/artist/album/art → cinder_set_now_playing(...)`; and library
  selection → gather `filename`s → build `NodeTrackSequence<UriInfo>` → `SetTrackSequence` (queue),
  with album grouping for shuffle-by-album.
- Net: the only piece that *needs* the libc++/IPC shim is **playback control**; the **entire
  library/metadata layer can be pure Rust + SQLite**. Big simplification.

## Open items
- Confirm `/db/MTPDB.dat` path + that it's a plain SQLite file on the device (`file`/`.tables`).
- Album-art blob storage: where `othumb_id`/`mthumb_id` resolve (a thumbs table / cache dir +
  offset/len); is embedded cover art also extractable.
- `media_type`/`object_type`/`format` enum values (which rows are audio tracks vs folders/playlists).
- Text encoding of `title`/`value` (TEXT = UTF-8 expected; some IPC getters are UTF-16).
- Playlists: `.m3u` handling (`ConvertM3uRootRule`) + how playlist membership is stored.

## Artifacts
SQL schema + symbols captured here. `libMediaStoreServiceClient.so` available to import into
`artifacts/ghidra_appmgr` if Path B enum values are needed.
