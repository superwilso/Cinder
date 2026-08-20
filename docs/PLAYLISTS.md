# Playlists you make on the device

*Added 2026-08-20. Cinder could browse and play Sony's playlists since 2026-07-26; it could not
make one. This is that half.*

## Why not write Sony's database

The MediaStore (`/db/MTPDB.dat`) holds playlists as container objects in a second object tree
(`analysis/H_mediastore/RE_findings.md`). Writing them would mean writing a database that

* is held open by Sony's own services while the player runs,
* is **rebuilt from scratch** whenever the library is rescanned, and
* **re-issues every `object_id`** when it is.

A playlist the owner curated by hand must survive all three. So Cinder keeps its own.

## The store

`/contents/cinder_playlists/*.m3u8` — one file per playlist, holding **file paths**, beside the
liked list at `/contents`, which is the volume Windows mounts.

```
#EXTM3U
#PLAYLIST:Late Night On The Bus      ← the display name, so it can hold anything a name can
#EXTINF:-1,Wunderhorse - Teal        ← readable label; never used for matching
/contents/MUSIC/Wunderhorse - Cub/06 - Wunderhorse - Teal.flac
```

Consequences, all deliberate:

* **A database rebuild cannot lose a playlist** — paths outlive object ids.
* **The PC can read and write them.** `.m3u8` is what `Sony sync` already handles, so a playlist
  made on the device opens in MusicBee, and one dropped into the folder shows up on the device.
* **It is NOT the music root.** The PC-side sync sweeps away playlists it did not plan for at
  `/contents/MUSIC` (`sync.py`, `MANAGED_PLAYLISTS`); this folder is out of its way.
* **Ids are negative.** `playlists::id_for` hashes the file stem; MediaStore ids are positive, so
  the sign alone tells `Action::PlayPlaylist(id)` which side to resolve — no second channel, and
  `PlayPlaylist` / `ShufflePlaylist` work for both kinds unchanged.
* Every write is a temp file + rename. This volume is exFAT on flash that gets unplugged.

Code: `player/cinder-ffi/src/playlists.rs` (store), `user_playlist_rows` / `refresh_playlists`
(merge into the UI's list), `add_track_to_playlist` (object id → path, via the DB).

## On screen

| where | what |
|---|---|
| **Library ▸ Playlists** | a **NEW PLAYLIST** row between the shuffle band and the list — the only way in, so it is a full-width row and it never scrolls away. Rows made here read "*n* tracks · YOURS". |
| **the playlist page** (yours only) | an edit bar: **+ TRACKS**, **RENAME**, **DELETE**, and a **×** on each row. |
| **× on a row** | two taps: the first arms the row and it says REMOVE?, the second removes. A tap anywhere else disarms. The same idiom as Settings ▸ Boot to stock, for the same reason. |
| **DELETE** | a yes/no modal (`confirm::Ask::DeletePlaylist`). The two-tap idiom is already spent on the ×, and "remove one track" and "delete the whole list" must not be the same gesture. |
| **+ TRACKS** | the library in title order, one tap adds one track, ticks show what is already in. The screen stays open — building a playlist is a run of taps. |
| **Now Playing ▸ toolbar slot 3** | "add what is playing to a playlist" → pick a playlist, or make one and it lands in there. |

Sony's playlists show the same page **without** the edit bar: the flag is `PlaylistRow::user`, and
a row that offered controls which silently did nothing would be worse than not offering them.

## The keyboard

`player/cinder-ui/src/keyboard.rs` — the device's first text input. There is no d-pad and no
hardware keyboard, so it is a touch grid: 10 keys of 40 px across the panel's 436 px of usable
width (the first cut used 44 px keys and the outer two were drawn as `…` because the shared text
helpers clamp to the 22 px gutter), QWERTY plus a numbers/symbols page, sticky CAPS, DEL, SPACE,
DONE. Back cancels — the same "leave without applying" as everywhere else, so there is no second
cancel button competing with Done.

`key_rect` is the single source for the render and the hit test, which is the one class of bug
this UI has shipped before (`AUDIT_2026-07-26.md` §F6b). Tests cover every key being where it is
drawn, the gaps typing nothing, the editing rules, and — in `tests/ui_overflow.rs` — every label
fitting its key at all seven UI scales.

## Not there yet

* **No search in the track picker.** It lists the whole library in title order; on the reference
  device that is 3,945 rows behind a scrollbar. The keyboard now exists, so a filter is a small
  job, but it is not done.
* **No "add this album/artist"** — the picker is per track.
* **No reordering** inside a playlist. Removing and re-adding is the only way to change the order.
* **`likesync` does not read this folder yet.** The files are ordinary `.m3u8`, so importing them
  into MusicBee is a copy; nothing automates it.
