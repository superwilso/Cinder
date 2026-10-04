# Ratings, play counts and saved views — the files Cinder keeps

*Added 2026-10-04 with the redesign's R4 pass ([`PLAN_redesign_2026-09.md`](PLAN_redesign_2026-09.md)).
Host-tested; nothing here has run on a player yet ([`DEVICE_CHECKLIST.md`](DEVICE_CHECKLIST.md) §25).*

Cinder now remembers three things Sony's database has no place for: the **rating** you gave a
track, **how often and when** you last played it, and your **saved views** (smart playlists). This
document says where each one lives and what the files look like, so a PC tool — Flint first — can
read them, and so you can back them up or edit them by hand.

## Where each thing comes from

| | Source | Written by Cinder? |
|---|---|---|
| Album artist | Sony's database (`object_body.albumartist_id` → `albumartists`), read by `cinder-db` | No. Read only |
| Rating | `/contents/cinder_stats.tsv` | Yes |
| Play count, last played | `/contents/cinder_stats.tsv` | Yes |
| Saved views | `/contents/cinder_views.conf` | Yes |
| "Edited on the player" mark | a line in the playlist's own `.m3u8` ([`PLAYLISTS.md`](PLAYLISTS.md)) | Yes |

**The album artist needed nothing new.** Sony's scanner already stores it and `cinder-db` already
reads it; the Albums and Artists tabs have grouped by it since 2026-08. What R4 adds is carrying it
on each song row (`SongRow::album_artist`, empty when it equals the track artist), which is what
shuffle by artist deals by.

**Cinder never writes Sony's database.** It is held open by Sony's services, rebuilt from scratch by
a rescan, and every `object_id` in it is re-issued when that happens. A rating has to outlive all
three, so it is keyed by **file path** in a file of Cinder's own — the same reasoning as the
playlists and the liked list.

Every file here is on `/contents`, the volume Windows mounts over USB, and every write goes to a
temporary file first and is then renamed over the old one. A cable pulled mid-write leaves the old
file or the new one, never half of either.

## `cinder_stats.tsv` — ratings and plays

```
#CINDER-STATS/1
# path<TAB>rating<TAB>plays<TAB>last_played
/contents/MUSIC/Wunderhorse - Cub/06 - Teal.flac<TAB>5<TAB>12<TAB>1790000000
/contents_ext/MUSIC/Nick Drake/Pink Moon/01 Pink Moon.flac<TAB>0<TAB>3<TAB>1789950000
```

(`<TAB>` is one tab character.)

| Column | Meaning |
|---|---|
| `path` | The file's path as the player sees it: `/contents/…` for internal storage, `/contents_ext/…` for the SD card. One file holds both volumes |
| `rating` | Stars, `0` to `5`. `0` means not rated |
| `plays` | How many times it has been listened to |
| `last_played` | Unix seconds of the last counted listen, `0` for never. Optional when writing by hand |

* A track with no rating and no plays has **no line**.
* Lines starting with `#` are comments. A line that does not read is skipped on its own; it never
  costs the rest of the file.
* The file is written in path order, so two saves of the same state are the same bytes.
* **Times.** `last_played` is the player's clock as an epoch, exactly as `.scrobbler.log` carries
  it: the player has no time zone, so it is local time labelled as UTC. Compare it only with other
  times from the same player. It is a 64-bit number in the code and plain decimal in the file, so
  nothing here wraps in 2038.

### What counts as a play

The scrobbler's rule (`scrobble::is_listened`): the track is over 30 seconds long and has played
for half its length or 4 minutes, whichever comes first. Paused time does not count. The count goes
up the moment that point is crossed, so a sudden power-off still keeps it.

Plays are counted **whether or not the scrobble log is being written**. The log is an install
option and can be handed to another scrobbler; the count is the library's own memory.

### When it is written

A change is written about three seconds after the last one, so five taps on the stars are one
write, and always before the volume is handed to a PC. Nothing is written while the PC has the
volume.

### If the file changes while the player holds it

Before each write Cinder checks whether the file is still the one it last read or wrote. If it is
not — a PC tool rewrote it, or the player started before `/contents` was readable and the real file
turned up afterwards — the two are **merged**:

* a track Cinder has not changed since its last write takes the file's values;
* a track Cinder did change keeps Cinder's rating (if the rating is what changed), and takes the
  larger play count and the later date.

So a PC tool may rewrite the file at any time the volume is mounted on the PC. To clear a rating
from the PC, write `0`; to remove a track's history, remove its line.

### For Flint

Reading is enough for a first version: the path maps to the file Flint copied, so ratings and play
counts can be shown in *On the player* and merged into a PC library. Writing back works today under
the merge rule above. Ratings are not sent to Last.fm (it has no such field); plays already travel
in `.scrobbler.log`.

## `cinder_views.conf` — saved views (smart playlists)

A saved view is a name, three rules and a sort. In the Library's Playlists tab it appears as a
**smart playlist**, above the ordinary ones, holding whatever its rules match at that moment. The
songs are not stored; only the rules are.

```
# Cinder saved views (smart playlists). Written by the player; see docs/TRACK_DATA.md.

[Late favourites]
rating=4
played=recent
format=flac
sort=plays
```

| Key | Values | Meaning |
|---|---|---|
| `[name]` | up to 48 characters, no `[` or `]` | Starts a view. Names are unique without regard to case; a second `[Mix]` is read as `Mix 2` |
| `rating` | `0`–`5` | Minimum stars. `0` = any, rated or not |
| `played` | `any` · `recent` · `not_lately` · `never` | `recent` = in the last 30 days. `not_lately` = played before, but not in the last 90 days. `never` = no counted play |
| `format` | `any` · `flac` · `mp3` · `m4a` · `hires` | By file extension; `hires` is the database's Hi-Res flag |
| `sort` | `title` · `plays` · `played` · `rating` · `added` | A to Z; most played first; most recently played first; highest rated first; newest in the library first |

* The rules are **and**ed together.
* A missing key, or a value that is not one of the words above, is that key's default (`0`, `any`,
  `any`, `title`). An unknown key is ignored. Lines before the first `[section]` are ignored.
* At most 32 views are read.
* If the file changed underneath, a view in the file that Cinder has never seen is kept when Cinder
  next saves. A view deleted on the player stays deleted.

The handoff's saved view also has *show as* and *pin to the Library bar*. Both belong to the
Library view bar, which is not built yet (R3), so the file has no keys for them. They will be added
as new keys, which older builds will ignore.

## Code

| | |
|---|---|
| The stats file | `player/cinder-ffi/src/stats.rs` (`Store`, `Listen`) |
| Stats for the UI, by object id | `cinder_ui::model::TrackStat`, `Library::stats`, `Library::album_rating` |
| Saved views | `player/cinder-ui/src/views.rs` (`SavedView`, `parse`, `serialize`) |
| Load and save | `cinder-ffi/src/lib.rs`: `load_views`, `save_views`, `flush_stats` |

## Tests

`cargo test -p cinder-ffi stats::` — the round trip, clearing, a damaged line, the merge, the listen
rule. `cargo test -p cinder-ui views::` — each rule, each sort, an unknown clock, the file's round
trip. `cargo test -p cinder-ui r4_tests` — rating, smart playlists and the playlist editor driven by
taps.
