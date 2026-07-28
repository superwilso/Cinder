//! cinder-db — read-only reader for the Sony media library SQLite DB (/db/MTPDB.dat).
//! Pure Rust (rusqlite, bundled SQLite); no libc++/IPC. Schema RE'd from
//! libMediaStoreService.so — see analysis/H_mediastore/RE_findings.md.
//!
//! Tables used: object_body (items/tracks), albums/artists (lookup, `value`=name),
//! object_ext_int (int props incl. DURATION, keyed by `akey`), schema (akey→prop_name),
//! images (album art: othumb_id/mthumb_id → images.id → bmpfile | (value,dataoffset,datasize)).

use rusqlite::{Connection, OpenFlags, Params, Result};

#[derive(Debug, Clone)]
pub struct Album {
    pub id: i64,
    pub name: String,
    pub track_count: i64,
}

#[derive(Debug, Clone)]
pub struct Artist {
    pub id: i64,
    pub name: String,
}

/// One playlist. `id` is the container's `object_id` (feed it to [`Db::playlist_tracks`]);
/// `track_count` counts only entries that still resolve to a playable track.
#[derive(Debug, Clone)]
pub struct Playlist {
    pub id: i64,
    pub name: String,
    pub track_count: i64,
}

#[derive(Debug, Clone)]
pub struct Track {
    pub object_id: i64,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub filename: String, // the URI/path PlayerService keys on
    pub disc_no: i64,
    pub track_no: i64,
    pub duration_raw: Option<i64>, // DURATION ext-int prop; units = DB's (calibrate on device, likely ms)
    pub is_hires: bool,
    pub othumb_id: Option<i64>, // -> images.id for album art
    pub album_id: Option<i64>,  // -> albums.id (stable key — album NAMES can collide)
    pub added: i64,             // object_body.addedtime (scan/import time; 0 if unknown) — "recently added"
    pub releaseyear_id: Option<i64>, // -> releaseyears.id (resolve via Db::release_years)
}

/// Album-art location for an object (from the `images` table). `value` is polymorphic in the
/// real MTPDB: TEXT = a source-file path (art embedded at dataoffset..+datasize), BLOB = the
/// raw image bytes stored inline. Reading it blindly as String made every BLOB row error out
/// of the whole query → art silently absent; the two shapes are split into separate fields.
#[derive(Debug, Clone)]
pub struct Art {
    pub bmpfile: Option<String>, // pre-rendered bitmap path, if present
    pub source_path: String,     // `value` when TEXT — file the art is embedded in / lives at
    pub blob: Option<Vec<u8>>,   // `value` when BLOB — the image bytes themselves
    pub data_offset: i64,
    pub data_size: i64,
    pub width: i64,
    pub height: i64,
}

#[derive(Debug, Clone, Copy)]
pub enum Sort {
    Title,
    Artist,
    Length,
    Added,
}

pub struct Db {
    conn: Connection,
    duration_akey: Option<i64>,
    /// object_id → absolute directory path, for every folder in the file tree. See `build_dirs`.
    dirs: std::collections::HashMap<i64, String>,
}

// Real tracks are object_body rows that have a file AND are audio. `filename IS NOT NULL` alone
// (the old filter) also matched folders (media_type=0: internal/MUSIC/LEARNING), cover images
// (Cover.jpg/*.png, media_type=3) and .m3u8 playlists (media_type=3) — 445 junk rows in a real
// device DB (RE 2026-07-25 on /db/MTPDB.dat). `media_type = 1` is Sony's semantic "audio" tag and
// is set on every playable track (and only those), so it's the correct, format-agnostic gate.
const TRACK_WHERE: &str = "ob.filename IS NOT NULL AND ob.media_type = 1";

impl Db {
    /// Open the library DB read-only (won't perturb the scanner's writes).
    pub fn open(path: &str) -> Result<Self> {
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        Ok(Self::wrap(conn))
    }

    /// In-memory DB (for tests / fixtures).
    pub fn open_in_memory() -> Result<Self> {
        Ok(Self::wrap(Connection::open_in_memory()?))
    }

    fn wrap(conn: Connection) -> Self {
        // Resolve the akey for the DURATION ext-int property once (None if absent).
        let duration_akey = conn
            .query_row(
                "SELECT akey FROM schema WHERE prop_name = 'DURATION' LIMIT 1",
                [],
                |r| r.get::<_, i64>(0),
            )
            .ok();
        let dirs = Self::build_dirs(&conn);
        Db { conn, duration_akey, dirs }
    }

    /// Mount point for each STORAGE ROOT of the MTP file tree, by the root object's name.
    ///
    /// The database stores `filename` as a BARE BASENAME — `01 A Horse with No Name.flac` — and the
    /// directories above it as separate `object_body` rows linked by `parent_id`. Nothing in the
    /// row tells you where the file actually is. Handing PlayerService the basename produced
    /// `content URI: /01 - … .flac`, which does not exist, so Sony's FLAC demuxer failed
    /// `WMX_CP_PIPETYPE::Open() (0x12)` → `GAP_E_INVALID_TRACK` and the play chain died the instant
    /// it was built. Every transport call still returned 0, which is why this looked like
    /// "starts then pauses" rather than an error.
    ///
    /// BOTH STORAGES MATTER: this device has ~2/3 of the library on internal storage and ~1/3 on
    /// the microSD, and they hang off two different roots.
    const ROOTS: [(&'static str, &'static str); 2] = [
        ("internal", "/contents"),      // /emmc@contents, vfat
        ("external", "/contents_ext"),  // the microSD, /dev/block/mmcblk1p1
    ];

    /// Resolve every folder object to an absolute directory path, once, at open.
    ///
    /// Walks `parent_id` up to a storage root and maps that root onto its mount point. Tracks are
    /// never parents, so only non-track rows are loaded (a few hundred, against thousands of
    /// tracks). Unknown roots resolve to nothing rather than guessing a mount — a track under one
    /// then keeps its bare filename, which is exactly the old behaviour and cannot make anything
    /// worse than it already was.
    fn build_dirs(conn: &Connection) -> std::collections::HashMap<i64, String> {
        use std::collections::HashMap;
        let mut raw: HashMap<i64, (i64, String)> = HashMap::new();
        if let Ok(mut st) = conn.prepare(
            "SELECT object_id, COALESCE(parent_id,0), COALESCE(filename, title, '') \
             FROM object_body WHERE media_type <> 1 OR filename IS NULL",
        ) {
            if let Ok(rows) = st.query_map([], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?))
            }) {
                for row in rows.flatten() {
                    raw.insert(row.0, (row.1, row.2));
                }
            }
        }
        // Resolve with memoisation; the depth is tiny (root/MUSIC/album) but a cycle in a corrupt
        // DB must not hang the boot, so the walk is bounded.
        let mut out: HashMap<i64, String> = HashMap::new();
        let ids: Vec<i64> = raw.keys().copied().collect();
        for id in ids {
            let mut stack: Vec<&str> = Vec::new();
            let mut cur = id;
            let mut prefix: Option<&str> = None;
            for _ in 0..32 {
                let Some((parent, name)) = raw.get(&cur) else { break };
                if *parent == 0 {
                    // A storage root: its own name selects the mount, and is not part of the path.
                    prefix = Self::ROOTS.iter().find(|(n, _)| n == name).map(|(_, m)| *m);
                    break;
                }
                stack.push(name.as_str());
                cur = *parent;
            }
            if let Some(mount) = prefix {
                let mut path = String::from(mount);
                for seg in stack.iter().rev() {
                    path.push('/');
                    path.push_str(seg);
                }
                out.insert(id, path);
            }
        }
        out
    }

    /// Absolute path for a track, from its parent folder plus its basename. `None` when the folder
    /// is under an unrecognised root.
    fn track_path(&self, parent_id: i64, basename: &str) -> Option<String> {
        let dir = self.dirs.get(&parent_id)?;
        Some(format!("{dir}/{basename}"))
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Albums with track counts, ordered for display.
    ///
    /// Albums with NO playable tracks are omitted. `albums` is a lookup table the stock scanner
    /// never prunes: deleting the music leaves the row behind, and the old correlated-subquery
    /// form listed those as real albums with "0 songs" (15 of 321 on the device library,
    /// 2026-07-26 — confirmed to have no `object_body` rows at all, not hidden ones we filtered
    /// out). An inner JOIN drops them by construction, and computes the count in the same pass
    /// instead of running a subquery per album row.
    pub fn albums(&self) -> Result<Vec<Album>> {
        let mut st = self.conn.prepare(
            &format!(
                "SELECT al.id, al.value, COUNT(ob.object_id) \
                 FROM albums al \
                 JOIN object_body ob ON ob.album_id = al.id AND {TRACK_WHERE} \
                 GROUP BY al.id, al.value, al.sort_str \
                 ORDER BY al.sort_str, al.value"
            ),
        )?;
        let rows = st.query_map([], |r| {
            Ok(Album { id: r.get(0)?, name: r.get(1)?, track_count: r.get(2)? })
        })?;
        rows.collect()
    }

    /// One representative track per album that actually has art, as `(album_id, object_id)`.
    ///
    /// This is what the shell's art-cache builder walks: covers are per-track in the schema
    /// (`object_body.othumb_id`), but every track on an album embeds the same picture, so one
    /// decode per album is enough. Ordered by album so a partial build is predictable.
    pub fn album_cover_sources(&self) -> Result<Vec<(i64, i64)>> {
        let mut st = self.conn.prepare(&format!(
            "SELECT ob.album_id, MIN(ob.object_id) \
             FROM object_body ob \
             WHERE {TRACK_WHERE} AND ob.album_id IS NOT NULL AND ob.othumb_id IS NOT NULL \
             GROUP BY ob.album_id \
             ORDER BY ob.album_id"
        ))?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect()
    }

    pub fn artists(&self) -> Result<Vec<Artist>> {
        let mut st = self
            .conn
            .prepare("SELECT id, value FROM artists ORDER BY sort_str, value")?;
        let rows = st.query_map([], |r| Ok(Artist { id: r.get(0)?, name: r.get(1)? }))?;
        rows.collect()
    }

    /// Playlists, alphabetically. There is **no playlist table** — Sony stores playlists as
    /// containers in a second object tree (RE'd 2026-07-26 against a real device DB):
    ///
    /// * the playlist itself is an `object_body` row with `object_type = 1`, its name in `title`,
    ///   and `filename` NULL (the `.m3u8` row in the *file* tree is a different object with
    ///   `child_count = 0` — it is the source file, not the membership, and is not what we read);
    /// * membership rows are `object_type = 3` with `parent_id` = the playlist, `reference_id` =
    ///   the track's `object_id`, and `child_index` = position.
    ///
    /// Detection is by shape (a container that has `object_type = 3` children) rather than by a
    /// hard-coded `tree_id`, so it can't break if the tree numbering differs on another unit —
    /// music folders are also `object_type = 1` but their children are type 2, never type 3.
    ///
    /// Deleting a playlist leaves its entries behind: on the reference DB **3028 of 3151 entry
    /// rows were orphans** pointing at parents that no longer exist. The parent join is therefore
    /// load-bearing, not decoration — without it the tab fills with ghost playlists.
    pub fn playlists(&self) -> Result<Vec<Playlist>> {
        let mut st = self.conn.prepare(
            "SELECT p.object_id, COALESCE(p.title,''), \
                    (SELECT COUNT(*) FROM object_body e \
                       JOIN object_body t ON t.object_id = e.reference_id AND t.media_type = 1 \
                      WHERE e.parent_id = p.object_id AND e.object_type = 3) \
             FROM object_body p \
             WHERE p.object_type = 1 \
               AND EXISTS (SELECT 1 FROM object_body e \
                            WHERE e.parent_id = p.object_id AND e.object_type = 3) \
             ORDER BY p.sort_str, p.title",
        )?;
        let rows = st.query_map([], |r| {
            Ok(Playlist { id: r.get(0)?, name: r.get(1)?, track_count: r.get(2)? })
        })?;
        rows.collect()
    }

    /// Tracks of one playlist, in the user's saved order (`child_index`).
    pub fn playlist_tracks(&self, playlist_id: i64) -> Result<Vec<Track>> {
        self.query_tracks(
            // The container join is what makes an id that isn't a live playlist return nothing:
            // orphaned entries keep their dead parent_id, so matching on parent_id alone would
            // resurrect a deleted playlist's tracks for whatever id it used to have.
            &format!(
                "JOIN object_body e ON e.reference_id = ob.object_id AND e.object_type = 3 \
                    AND e.parent_id = ?1 \
                 JOIN object_body p ON p.object_id = e.parent_id AND p.object_type = 1 \
                 WHERE {TRACK_WHERE}"
            ),
            "e.child_index",
            [playlist_id],
        )
    }

    /// Tracks of one album, in disc/track order.
    pub fn album_tracks(&self, album_id: i64) -> Result<Vec<Track>> {
        self.query_tracks(
            &format!("WHERE {TRACK_WHERE} AND ob.album_id = ?1"),
            "ob.disc_no, ob.series_no, ob.child_index",
            [album_id],
        )
    }

    /// Every track in (album, disc, track) order — one query to build all the per-album track
    /// lists (group consecutive `album_id` runs), instead of a query per album.
    pub fn tracks_album_order(&self) -> Result<Vec<Track>> {
        self.query_tracks(
            &format!("WHERE {TRACK_WHERE}"),
            "ob.album_id, ob.disc_no, ob.series_no, ob.child_index",
            [],
        )
    }

    /// All tracks, sorted for the Songs tab.
    pub fn tracks(&self, sort: Sort) -> Result<Vec<Track>> {
        let order = match sort {
            Sort::Title => "ob.sort_str, ob.title",
            Sort::Artist => "ar.sort_str, ar.value, ob.disc_no, ob.series_no",
            Sort::Length => "dur.value, ob.title",
            Sort::Added => "ob.addedtime DESC, ob.title",
        };
        self.query_tracks(&format!("WHERE {TRACK_WHERE}"), order, [])
    }

    /// Resolve the now-playing metadata for the file PlayerService reports (PlayStatus.uri).
    pub fn track_by_filename(&self, filename: &str) -> Result<Option<Track>> {
        // PlayerService reports back the absolute path we gave it, but the DB column is a bare
        // basename — so match on the basename and then disambiguate on the full path. Basenames
        // repeat constantly across a real library ("01 Intro.flac"), so picking the first row that
        // merely shares a name would show the wrong album's metadata on Now Playing.
        let base = filename.rsplit('/').next().unwrap_or(filename);
        let v = self.query_tracks(
            &format!("WHERE {TRACK_WHERE} AND ob.filename = ?1"),
            "ob.object_id",
            [base],
        )?;
        if let Some(exact) = v.iter().position(|t| t.filename == filename) {
            return Ok(v.into_iter().nth(exact));
        }
        // No exact match: either the caller passed a bare basename (older callers did), or the
        // track is under an unresolved root. Either way the single-candidate answer is still right.
        Ok(v.into_iter().next())
    }

    /// The play context for a chosen track: its album's tracks in disc/track order plus the
    /// track's index within them. Falls back to just the track itself if it has no album (or
    /// somehow isn't in its own album's list). This is what "tap a song" hands PlayerService,
    /// so Next/Prev then move within the album — stock behavior.
    pub fn album_context(&self, object_id: i64) -> Result<Option<(Vec<Track>, usize)>> {
        let album_id: Option<i64> = self
            .conn
            .query_row(
                "SELECT album_id FROM object_body WHERE object_id = ?1",
                [object_id],
                |r| r.get(0),
            )
            .unwrap_or(None);
        let tracks = match album_id {
            Some(id) => self.album_tracks(id)?,
            None => Vec::new(),
        };
        if let Some(idx) = tracks.iter().position(|t| t.object_id == object_id) {
            return Ok(Some((tracks, idx)));
        }
        // No album row (or the track vanished from it): play the single track.
        let one = self.query_tracks(
            &format!("WHERE {TRACK_WHERE} AND ob.object_id = ?1"),
            "ob.object_id",
            [object_id],
        )?;
        Ok(one.into_iter().next().map(|t| (vec![t], 0)))
    }

    /// Album-art record for an object (via its othumb_id → images), if any.
    pub fn art_for_object(&self, object_id: i64) -> Result<Option<Art>> {
        let mut st = self.conn.prepare(
            "SELECT im.bmpfile, im.value, im.dataoffset, im.datasize, im.bmpwidth, im.bmpheight \
             FROM object_body ob JOIN images im ON im.id = ob.othumb_id \
             WHERE ob.object_id = ?1",
        )?;
        let mut rows = st.query([object_id])?;
        match rows.next()? {
            Some(r) => {
                // `value` is TEXT (a path) or BLOB (inline image bytes) depending on how the
                // stock scanner stored this cover — inspect the actual storage class.
                let (source_path, blob) = match r.get_ref(1)? {
                    rusqlite::types::ValueRef::Text(t) => {
                        (String::from_utf8_lossy(t).into_owned(), None)
                    }
                    rusqlite::types::ValueRef::Blob(b) => (String::new(), Some(b.to_vec())),
                    _ => (String::new(), None),
                };
                Ok(Some(Art {
                    bmpfile: r.get::<_, Option<String>>(0)?,
                    source_path,
                    blob,
                    data_offset: r.get::<_, Option<i64>>(2)?.unwrap_or(0),
                    data_size: r.get::<_, Option<i64>>(3)?.unwrap_or(0),
                    width: r.get::<_, Option<i64>>(4)?.unwrap_or(0),
                    height: r.get::<_, Option<i64>>(5)?.unwrap_or(0),
                }))
            }
            None => Ok(None),
        }
    }

    /// Shared track SELECT: object_body + artist/album names + optional DURATION ext-int.
    /// The DURATION akey is a trusted integer resolved from the DB, so it's inlined into the
    /// SQL (no param-ordering games); the only bound params are the WHERE clause's.
    fn query_tracks<P: Params>(
        &self,
        where_clause: &str,
        order_by: &str,
        params: P,
    ) -> Result<Vec<Track>> {
        let (dur_join, dur_sel) = match self.duration_akey {
            Some(akey) => (
                format!(
                    "LEFT JOIN object_ext_int dur \
                     ON dur.object_id = ob.object_id AND dur.akey = {akey}"
                ),
                "dur.value",
            ),
            None => (String::new(), "NULL"),
        };
        let sql = format!(
            "SELECT ob.object_id, ob.title, COALESCE(ar.value,''), COALESCE(al.value,''), \
                    ob.filename, COALESCE(ob.disc_no,0), COALESCE(ob.series_no,0), {dur_sel}, \
                    COALESCE(ob.is_high_resolution,0), ob.othumb_id, ob.album_id, \
                    COALESCE(ob.addedtime,0), ob.releaseyear_id, COALESCE(ob.parent_id,0) \
             FROM object_body ob \
             LEFT JOIN artists ar ON ar.id = ob.artist_id \
             LEFT JOIN albums  al ON al.id = ob.album_id \
             {dur_join} {where_clause} ORDER BY {order_by}"
        );
        let mut st = self.conn.prepare(&sql)?;
        let rows = st.query_map(params, |r| {
            Ok(Track {
                object_id: r.get(0)?,
                title: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                artist: r.get(2)?,
                album: r.get(3)?,
                // The DB's `filename` is a BARE BASENAME; the directories live in the parent
                // chain. Resolve to the absolute path here so every consumer — play_tracks,
                // now-playing lookup, the queue — gets something that actually opens. Falls back
                // to the basename when the folder sits under an unrecognised root, which is the
                // pre-2026-07-28 behaviour rather than a new failure.
                filename: {
                    let base = r.get::<_, Option<String>>(4)?.unwrap_or_default();
                    let parent: i64 = r.get(13)?;
                    self.track_path(parent, &base).unwrap_or(base)
                },
                disc_no: r.get(5)?,
                track_no: r.get(6)?,
                duration_raw: r.get(7)?,
                is_hires: r.get::<_, i64>(8)? != 0,
                othumb_id: r.get(9)?,
                album_id: r.get(10)?,
                added: r.get(11)?,
                releaseyear_id: r.get(12)?,
            })
        })?;
        rows.collect()
    }

    /// Resolve `releaseyear_id` → the display year string, best-effort. The MediaStore's release-year
    /// lookup table wasn't captured verbatim in RE, but every sibling lookup (albums/artists/genres)
    /// is `(id PK, value, sort_str, …)` and the FK stems pluralize (`album_id`→`albums`,
    /// `genre_id`→`genres`), so `releaseyear_id` → `releaseyears(id, value)`. We try that name, then
    /// the singular `releaseyear`, and return an EMPTY map on any error — a missing/differently-shaped
    /// table just leaves years blank (exactly today's behavior), never fails the library build. The
    /// count is logged once so a device DB pull tells us whether the guess held.
    pub fn release_years(&self) -> std::collections::HashMap<i64, String> {
        for table in ["releaseyears", "releaseyear"] {
            let sql = format!("SELECT id, value FROM {table}");
            let mut st = match self.conn.prepare(&sql) {
                Ok(s) => s,
                Err(_) => continue, // table doesn't exist under this name — try the next
            };
            let rows = st.query_map([], |r| {
                let id: i64 = r.get(0)?;
                // `value` is normally the year TEXT ("2019"); tolerate an INTEGER column too.
                let year = match r.get_ref(1)? {
                    rusqlite::types::ValueRef::Text(t) => String::from_utf8_lossy(t).into_owned(),
                    rusqlite::types::ValueRef::Integer(n) => n.to_string(),
                    _ => String::new(),
                };
                Ok((id, year))
            });
            if let Ok(rows) = rows {
                let map: std::collections::HashMap<i64, String> =
                    rows.filter_map(|r| r.ok()).filter(|(_, y)| !y.is_empty()).collect();
                eprintln!("[cinder-db] release_years: table '{table}' -> {} entries", map.len());
                return map;
            }
        }
        eprintln!("[cinder-db] release_years: no releaseyears/releaseyear table — years left blank");
        std::collections::HashMap::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fixture DB built with the (RE'd) schema + sample rows.
    fn db() -> Db {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE albums  (id INTEGER PRIMARY KEY, initial INTEGER, sort_str TEXT, search_str TEXT, value TEXT);
            CREATE TABLE artists (id INTEGER PRIMARY KEY, initial INTEGER, sort_str TEXT, search_str TEXT, value TEXT, imagefile TEXT, face_x INTEGER, face_y INTEGER, face_w INTEGER, face_h INTEGER);
            CREATE TABLE schema  (prop_type INTEGER, akey INTEGER, data_type INTEGER, prop_name TEXT, PRIMARY KEY(prop_type,akey));
            CREATE TABLE object_ext_int (object_id INTEGER, akey INTEGER, value INTEGER DEFAULT 0, PRIMARY KEY(object_id,akey));
            CREATE TABLE images  (id INTEGER PRIMARY KEY, dataform INTEGER, dataoffset INTEGER, datasize INTEGER, value TEXT, digest TEXT, bmpfile TEXT, bmpwidth INTEGER, bmpheight INTEGER);
            CREATE TABLE releaseyears (id INTEGER PRIMARY KEY, initial INTEGER, sort_str TEXT, search_str TEXT, value TEXT);
            CREATE TABLE object_body (
                object_id INTEGER PRIMARY KEY AUTOINCREMENT, object_type INTEGER NOT NULL,
                parent_id INTEGER, reference_id INTEGER,
                child_index INTEGER, media_type INTEGER DEFAULT 0, format INTEGER DEFAULT 0,
                initial INTEGER, sort_str TEXT, search_str TEXT, title TEXT DEFAULT "",
                addedtime INTEGER DEFAULT 0, filename TEXT, filesize INTEGER,
                series_no INTEGER, disc_no INTEGER, is_high_resolution INTEGER,
                album_id INTEGER, artist_id INTEGER, releaseyear_id INTEGER, othumb_id INTEGER, mthumb_id INTEGER);
            INSERT INTO albums  VALUES (10,0,'last smoke','last smoke','Last Smoke Before the Snowstorm');
            INSERT INTO albums  VALUES (11,0,'harvest','harvest','Harvest Moon');
            -- Orphan lookup row: the stock scanner leaves these behind when the music is deleted,
            -- and they used to surface in the UI as real albums with "0 songs".
            INSERT INTO albums  VALUES (12,0,'ghost','ghost','Deleted Album');
            INSERT INTO artists VALUES (20,0,'leftwich','leftwich','Benjamin Francis Leftwich',NULL,0,0,0,0);
            INSERT INTO artists VALUES (21,0,'cold','cold','Cold Stone & Sea',NULL,0,0,0,0);
            INSERT INTO schema  VALUES (1,7,2,'DURATION');
            INSERT INTO images  VALUES (100,0,4096,20000,'/music/atlas.flac','d1','/db/thumb/100.bmp',92,92);
            INSERT INTO releaseyears VALUES (30,0,'2012','2012','2012');
            INSERT INTO releaseyears VALUES (31,0,'1992','1992','1992');
            -- THE FILE TREE, exactly as the device stores it. `filename` is a BARE BASENAME and the
            -- directories are separate rows linked by parent_id; the storage ROOTS (parent_id 0)
            -- select the mount. Modelling this correctly is not decoration — the old fixture put
            -- absolute paths in `filename`, which no real DB does, and that is precisely why the
            -- "hand PlayerService a path with no directory" bug survived every test until 07-28.
            -- BOTH storages are represented: this device keeps roughly a third of the library on
            -- the microSD, under a separate root.
            INSERT INTO object_body (object_id,object_type,parent_id,media_type,title,filename) VALUES (900,1,0,0,'internal','internal');
            INSERT INTO object_body (object_id,object_type,parent_id,media_type,title,filename) VALUES (901,1,900,0,'MUSIC','MUSIC');
            INSERT INTO object_body (object_id,object_type,parent_id,media_type,title,filename) VALUES (902,1,901,0,'Benjamin Francis Leftwich - Last Smoke','Benjamin Francis Leftwich - Last Smoke');
            INSERT INTO object_body (object_id,object_type,parent_id,media_type,title,filename) VALUES (910,1,0,0,'external','external');
            INSERT INTO object_body (object_id,object_type,parent_id,media_type,title,filename) VALUES (911,1,910,0,'MUSIC','MUSIC');
            INSERT INTO object_body (object_id,object_type,parent_id,media_type,title,filename) VALUES (912,1,911,0,'Neil Young - Harvest Moon','Neil Young - Harvest Moon');
            -- An unrecognised root: tracks under it must degrade to the bare basename, not guess.
            INSERT INTO object_body (object_id,object_type,parent_id,media_type,title,filename) VALUES (920,1,0,0,'limited','limited');
            INSERT INTO object_body (object_id,object_type,parent_id,media_type,title,filename) VALUES (921,1,920,0,'ODD','ODD');
            INSERT INTO object_body (object_id,object_type,parent_id,media_type,child_index,title,filename,series_no,disc_no,is_high_resolution,album_id,artist_id,releaseyear_id,othumb_id,addedtime)
              VALUES (1,1,902,1,0,'Atlas Hands','atlas.flac',1,1,1,10,20,30,100,5000);
            INSERT INTO object_body (object_id,object_type,parent_id,media_type,child_index,title,filename,series_no,disc_no,is_high_resolution,album_id,artist_id,releaseyear_id,othumb_id,addedtime)
              VALUES (2,1,902,1,1,'Box of Stones','box.flac',2,1,1,10,20,30,NULL,5001);
            -- On the SD CARD, and deliberately sharing a basename with the internal track below it
            -- would be ambiguous — the now-playing lookup must disambiguate on the FULL path.
            INSERT INTO object_body (object_id,object_type,parent_id,media_type,child_index,title,filename,series_no,disc_no,is_high_resolution,album_id,artist_id,releaseyear_id,othumb_id,addedtime)
              VALUES (3,1,912,1,0,'Harvest Moon','harvest.flac',1,1,0,11,21,31,NULL,4000);
            -- a folder (media_type 0) and a stray cover image (media_type 3) — both must be excluded
            INSERT INTO object_body (object_id,object_type,media_type,title,filename,album_id) VALUES (9,0,0,'A Folder',NULL,NULL);
            INSERT INTO object_body (object_id,object_type,media_type,child_index,title,filename,album_id) VALUES (8,2,3,0,'Cover','Cover.jpg',10);
            -- A playlist: a container (type 1) whose children are type-3 entries pointing at
            -- tracks by reference_id, ordered by child_index (NOT title order).
            INSERT INTO object_body (object_id,object_type,parent_id,media_type,title,filename) VALUES (50,1,0,0,'Night Bus',NULL);
            INSERT INTO object_body (object_id,object_type,parent_id,reference_id,child_index) VALUES (51,3,50,3,0);
            INSERT INTO object_body (object_id,object_type,parent_id,reference_id,child_index) VALUES (52,3,50,1,1);
            -- Orphan entry: its parent playlist was deleted. Must NOT produce a ghost playlist
            -- (3028 of 3151 entries were orphans on the real device DB).
            INSERT INTO object_body (object_id,object_type,parent_id,reference_id,child_index) VALUES (53,3,999,2,0);
            INSERT INTO object_ext_int VALUES (1,7,272000);
            INSERT INTO object_ext_int VALUES (3,7,303000);
            "#,
        ).unwrap();
        Db::wrap(conn)
    }

    #[test]
    fn playlists_found_by_shape_and_orphans_ignored() {
        let p = db().playlists().unwrap();
        // Only the real container. The orphan entry's parent (999) must not become a playlist,
        // and plain tracks (also object_type 1 in this fixture) must not either — the
        // has-type-3-children shape is what separates them.
        assert_eq!(p.len(), 1, "got {p:?}");
        assert_eq!(p[0].name, "Night Bus");
        assert_eq!(p[0].id, 50);
        assert_eq!(p[0].track_count, 2);
    }

    #[test]
    fn playlist_tracks_keep_saved_order() {
        let t = db().playlist_tracks(50).unwrap();
        // child_index order, not alphabetical: Harvest Moon (idx 0) then Atlas Hands (idx 1).
        let names: Vec<&str> = t.iter().map(|x| x.title.as_str()).collect();
        assert_eq!(names, vec!["Harvest Moon", "Atlas Hands"]);
    }

    /// 999 is the dead parent of the orphan entry, so this also proves a deleted playlist's
    /// tracks can't be resurrected by asking for its old id.
    #[test]
    fn playlist_tracks_of_unknown_playlist_is_empty() {
        assert!(db().playlist_tracks(999).unwrap().is_empty());
    }

    #[test]
    fn duration_akey_resolved() {
        assert_eq!(db().duration_akey, Some(7));
    }

    #[test]
    fn albums_with_counts() {
        let a = db().albums().unwrap();
        // 3 rows in `albums`, but id 12 has no tracks left and must not be listed.
        assert_eq!(a.len(), 2);
        assert!(a.iter().all(|x| x.id != 12), "orphan album row listed: {a:?}");
        assert!(a.iter().all(|x| x.track_count > 0), "album with 0 tracks listed: {a:?}");
        let last = a.iter().find(|x| x.id == 10).unwrap();
        assert_eq!(last.name, "Last Smoke Before the Snowstorm");
        assert_eq!(last.track_count, 2); // Atlas + Box (folder excluded)
    }

    #[test]
    fn album_tracks_ordered_with_duration() {
        let t = db().album_tracks(10).unwrap();
        assert_eq!(t.len(), 2);
        assert_eq!(t[0].title, "Atlas Hands");
        assert_eq!(t[0].artist, "Benjamin Francis Leftwich");
        assert_eq!(t[0].duration_raw, Some(272000));
        assert!(t[0].is_hires);
        assert_eq!(t[1].title, "Box of Stones");
    }

    #[test]
    fn tracks_sorted_by_title_excludes_folder() {
        let t = db().tracks(Sort::Title).unwrap();
        let titles: Vec<_> = t.iter().map(|x| x.title.as_str()).collect();
        assert_eq!(titles, ["Atlas Hands", "Box of Stones", "Harvest Moon"]);
    }

    /// THE bug of 2026-07-28: the DB stores a bare basename, and handing that to PlayerService
    /// produced `content URI: /01 - ….flac` — a path with no directory. Sony's demuxer failed to
    /// open it, the play chain died the instant it was built, and because every transport call
    /// still returned 0 it looked like "starts, then pauses" rather than an error.
    ///
    /// BOTH storages must resolve: roughly a third of a real library lives on the microSD, under a
    /// different root that maps to a different mount.
    #[test]
    fn track_paths_resolve_absolutely_on_both_storages() {
        let d = db();
        let all = d.tracks(Sort::Title).unwrap();
        let by_title = |t: &str| all.iter().find(|x| x.title == t).cloned().unwrap();

        // Internal storage → /contents
        let atlas = by_title("Atlas Hands");
        assert_eq!(
            atlas.filename,
            "/contents/MUSIC/Benjamin Francis Leftwich - Last Smoke/atlas.flac"
        );

        // microSD → /contents_ext
        let harvest = by_title("Harvest Moon");
        assert_eq!(
            harvest.filename,
            "/contents_ext/MUSIC/Neil Young - Harvest Moon/harvest.flac"
        );

        // Every resolved path must be absolute and must carry a directory — a bare "/name.flac"
        // is the exact shape that failed on device.
        for t in &all {
            assert!(t.filename.starts_with('/'), "not absolute: {}", t.filename);
            assert!(
                t.filename.matches('/').count() >= 2,
                "no directory component, this is the on-device failure: {}",
                t.filename
            );
        }
    }

    /// An unrecognised storage root must NOT be guessed at. Falling back to the bare basename is
    /// the pre-fix behaviour: no worse than before, and it cannot send PlayerService to a path on
    /// the wrong volume.
    #[test]
    fn an_unknown_root_degrades_instead_of_guessing() {
        let d = db();
        d.conn()
            .execute_batch(
                "INSERT INTO object_body (object_id,object_type,parent_id,media_type,child_index,\
                   title,filename,series_no,disc_no,album_id) \
                 VALUES (4,1,921,1,0,'Odd One','odd.flac',1,1,10);",
            )
            .unwrap();
        let t = d.tracks(Sort::Title).unwrap().into_iter().find(|t| t.title == "Odd One").unwrap();
        assert_eq!(t.filename, "odd.flac", "an unknown root must not invent a mount");
    }

    /// Basenames repeat constantly across a real library. The now-playing lookup gets the absolute
    /// path back from PlayerService and must disambiguate on it, or Now Playing shows the wrong
    /// album's metadata for a track that merely shares a filename.
    #[test]
    fn now_playing_disambiguates_duplicate_basenames_across_storages() {
        let d = db();
        // Same basename as the SD-card "harvest.flac", but on internal storage in another album.
        d.conn()
            .execute_batch(
                "INSERT INTO object_body (object_id,object_type,parent_id,media_type,child_index,\
                   title,filename,series_no,disc_no,album_id) \
                 VALUES (5,1,902,1,2,'Harvest Moon (Live)','harvest.flac',3,1,10);",
            )
            .unwrap();
        let sd = d
            .track_by_filename("/contents_ext/MUSIC/Neil Young - Harvest Moon/harvest.flac")
            .unwrap()
            .unwrap();
        assert_eq!(sd.title, "Harvest Moon");
        let internal = d
            .track_by_filename("/contents/MUSIC/Benjamin Francis Leftwich - Last Smoke/harvest.flac")
            .unwrap()
            .unwrap();
        assert_eq!(internal.title, "Harvest Moon (Live)");
    }

    #[test]
    fn now_playing_lookup_by_filename() {
        let t = db().track_by_filename("/contents_ext/MUSIC/Neil Young - Harvest Moon/harvest.flac").unwrap().unwrap();
        assert_eq!(t.title, "Harvest Moon");
        assert_eq!(t.album, "Harvest Moon");
        assert_eq!(t.duration_raw, Some(303000));
    }

    #[test]
    fn addedtime_and_release_year_resolve() {
        let d = db();
        let t = d.album_tracks(10).unwrap();
        assert_eq!(t[0].added, 5000);
        assert_eq!(t[0].releaseyear_id, Some(30));
        let years = d.release_years();
        assert_eq!(years.get(&30).map(|s| s.as_str()), Some("2012"));
        assert_eq!(years.get(&31).map(|s| s.as_str()), Some("1992"));
    }

    #[test]
    fn release_years_missing_table_is_empty_not_error() {
        // A DB without the releaseyears table must degrade to an empty map, never panic.
        let conn = Connection::open_in_memory().unwrap();
        let d = Db::wrap(conn);
        assert!(d.release_years().is_empty());
    }

    #[test]
    fn art_lookup() {
        let d = db();
        let art = d.art_for_object(1).unwrap().unwrap();
        assert_eq!(art.bmpfile.as_deref(), Some("/db/thumb/100.bmp"));
        assert_eq!(art.data_offset, 4096);
        assert_eq!(art.width, 92);
        assert!(d.art_for_object(2).unwrap().is_none()); // no othumb
    }
}
