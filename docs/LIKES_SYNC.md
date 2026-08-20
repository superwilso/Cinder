# Liked songs — the device ⇄ PC contract

*Added 2026-08-20 alongside the PC-side `likesync` tool (repo: `Sony sync`).*

Cinder has had liked songs since 2026-07-27 (`cinder_liked.conf` + the Now Playing heart + the
Library's "Liked songs" row). What it did not have is a way for a like to arrive **from** the PC,
which is what a sync needs — the device is the only participant that cannot reach the network, so
every path between it, Last.fm and MusicBee has to be a file.

## The three files

| path | direction | format | written by |
|---|---|---|---|
| `/contents/cinder_liked.conf` | — | one MediaStore `object_id` per line | Cinder (the real store) |
| `/contents/cinder_loved.tsv` | device → PC | `artist \t title`, `#` header | Cinder, on every liked change |
| `/contents/cinder_liked_import.tsv` | PC → device | `artist \t title`, `# artist…` header | `likesync` |

Object ids are rebuilt whenever the MTP database is, so they cannot cross the USB cable — hence
the two TSVs. `/contents` is the volume Windows mounts, so all three are readable and editable by
hand over USB-MSC, which matters for a list the owner curated by hand and cannot otherwise back up.

## What Cinder does with the import

`player/cinder-ffi/src/likes.rs`, called from `cinder_db_open` right after the library is built:

1. read `/contents/cinder_liked_import.tsv`; absent → nothing happens (the normal boot);
2. **no `# artist` header → ignore and leave it in place** (it is not ours);
3. resolve each row against the library just built — artist+title normalised the same way the PC
   normalises (case, curly punctuation, `feat.` credits, re-issue suffixes such as
   `- Remastered 2009` / `(2021 Mix)`), then by primary artist, then by **album artist** (a
   featured-artist track is tagged to the guest: `Cleo Sol — Woman` on a Little Simz album);
4. **rows present but nothing resolved → ignore and leave it in place**, so the next boot retries
   (that shape means the library had not loaded, not that the list is empty);
5. otherwise **replace** the liked set with the resolved ids, rewrite `cinder_liked.conf` and the
   `cinder_loved.tsv` export, and rename the import to `…​.tsv.done`.

**Replace, not merge**, because the import is the merged whole list, not a delta — a track missing
from it was deliberately unliked somewhere, and merging would make an unlike impossible to express.
An empty file *with* the header therefore means "everything was unliked" and is honoured; the PC
writes it atomically, so a zero-row file cannot be a torn write.

The rename is the signal back to the PC: while the file exists, `likesync` treats the device as
additive-only, because its export still shows the pre-push list. A Cinder build without this
feature never renames it, so an un-updated device keeps working — it just never gains hearts from
the PC, and `likesync` says so.

## What is NOT here

* No network on the device. There is no WiFi on an NW-A55, and `track.love` cannot be expressed in
  an Audioscrobbler/1.1 log either: its rating column is `L` = *Listened* / `S` = *Skipped*, which
  is not a love. That is why loves need their own file rather than riding the scrobbler log.
* No liked-songs *playlist* written by the device. `likesync` writes
  `/contents/MUSIC/Liked Songs.m3u8` per volume, which Sony's indexer picks up like any other
  playlist, so it plays without a firmware change.

## Tests

`cargo test -p cinder-ffi likes::` — 11 cases: the re-issue folding, live/remix staying distinct,
the featured-artist and primary-artist fallbacks, junk lines, the missing header, the unresolved
refusal, the empty-clear, and the rename.
