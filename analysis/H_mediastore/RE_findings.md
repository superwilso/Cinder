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
- ~~Playlists: how playlist membership is stored.~~ **CLOSED 2026-07-26** — see below.

## Playlists — SOLVED (2026-07-26, against a real device DB)

Verified on `artifacts/MTPDB_dev.dat` (pulled 2026-07-25; 6949 objects, 4 playlists).
**There is no playlist table.** Playlists live in `object_body` itself, in a *second object tree*:

| Role | Row shape |
|---|---|
| The playlist | `object_type = 1`, `title` = its name, `filename` NULL, `parent_id = 0`, `child_count` = N |
| A member | `object_type = 3`, `parent_id` = the playlist's `object_id`, `reference_id` = the **track's** `object_id`, `child_index` = position |

On the reference DB the music/file tree is `tree_id = 1` and the playlist tree is `tree_id = 19`,
but **do not key on `tree_id`** — detect by shape instead: a playlist is a container that has
`object_type = 3` children. Music folders are also `object_type = 1`, but their children are type
2, never type 3, so the shape rule separates them and cannot break if tree numbering differs on
another unit.

Two traps, both real on this DB:

1. **The `.m3u8` rows are decoys.** Each playlist also has a row in the *file* tree
   (`object_type = 2`, `format = 12`, `parent_id = 4` = MUSIC, filename `*.m3u8`) — but its
   `child_count` is **0** and it has no children. That row is the source file, not the membership.
   Reading it gets you playlist names with zero tracks.
2. **Deleting a playlist orphans its entries.** 3028 of the 3151 `object_type = 3` rows point at a
   `parent_id` that no longer exists — 96% garbage. Joining to the container is what keeps deleted
   playlists (and their tracks) from coming back; matching on `parent_id` alone resurrects them.

Both are covered by tests in `player/cinder-db/src/lib.rs` (`playlists_found_by_shape_and_orphans_ignored`,
`playlist_tracks_of_unknown_playlist_is_empty`). API: `Db::playlists()` → `Playlist { id, name,
track_count }` (count = entries that still resolve to a playable track), `Db::playlist_tracks(id)`
→ tracks in saved `child_index` order. Inspect any DB with
`cargo run -p cinder-db --example playlists -- <MTPDB.dat>` (or `--example schema_dump` for the
whole schema).

Still open: `.m3u` *writing* (`ConvertM3uRootRule`) if Cinder ever edits playlists — reading is
done.

## Artifacts
SQL schema + symbols captured here. `libMediaStoreServiceClient.so` available to import into
`artifacts/ghidra_appmgr` if Path B enum values are needed.

---

## 2026-08-16 — the genre / composer / release-year columns, settled from the live DB

Pulled `/db/MTPDB.dat` off the device (5.2 MB, 19 tables) and read the schema directly rather than
inferring it. Everything the earlier rounds guessed at or deferred is in **one place**: `object_body`
already carries the foreign keys.

```sql
CREATE TABLE object_body ( object_id INTEGER PRIMARY KEY AUTOINCREMENT, … ,
    is_high_resolution INTEGER, … ,
    album_id INTEGER, artist_id INTEGER, albumartist_id INTEGER, composer_id INTEGER,
    genre_id INTEGER, videogenre_id INTEGER, releaseyear_id INTEGER, rating_id INTEGER,
    othumb_id INTEGER, mthumb_id INTEGER )
```

Lookup tables, all the same shape as `albums`/`artists`:

| Table | Columns | Rows on the reference device |
|---|---|---|
| `genres` | `id, initial, sort_str, value UNIQUE` | 101 |
| `composers` | `id, initial, sort_str, search_str, value UNIQUE` | 290 |
| `releaseyears` | `id, value INTEGER UNIQUE` | 68 |
| `albumartists` | `id, initial, sort_str, search_str, value UNIQUE` | 189 |

**`releaseyears` is CONFIRMED.** `Db::release_years` has been trying `releaseyears` then
`releaseyear` since 2026-07-03 with a comment saying the name was never captured in RE. It is
`releaseyears`, and its `value` is an **INTEGER**, not text — the existing reader already tolerates
both, so nothing was wrong, but the fallback is now known to be dead code rather than insurance.

### What the data actually looks like (3,463 tracks)

- **`genre_id` is never NULL.** "No genre" is a real row whose `value` is the empty string, and it
  is the **single largest bucket at 482 tracks** — 14% of the library. A filter that drops it, or
  that treats missing-genre as NULL, silently cannot reach an eighth of the library.
- **95 of the 101 genres are carried by a track.** Count from the tracks, not from the table, or the
  picker offers six choices that match nothing.
- Genres are not all Latin: `語学`, `Électronique`. The `text.rs` device-font fallback covers them.
- **`is_high_resolution` is set on exactly 1 of 3,463 tracks** on this device. The column works; a
  Hi-Res filter is simply near-inert on this particular library.
- 275 distinct composers in use — enough for a composer filter later, and the column is right there.

### One robustness consequence

Naming `ob.genre_id` unconditionally in the track SELECT would make the **whole** track query fail
on any firmware variant whose `object_body` lacks it — turning "no genre filter" into "no library".
`Db` now probes for the column once at open (`has_genre`) and selects `NULL` instead, the same shape
`albumartist_table` already used. Covered by `genres_missing_table_is_empty_not_error`.
