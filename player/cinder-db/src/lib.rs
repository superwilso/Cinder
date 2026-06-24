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
}

/// Album-art location for an object (from the `images` table).
#[derive(Debug, Clone)]
pub struct Art {
    pub bmpfile: Option<String>, // pre-rendered bitmap path, if present
    pub source_path: String,     // `value` — file the art is embedded in / lives at
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
}

// Real tracks are object_body rows that have a file (folders/containers don't).
const TRACK_WHERE: &str = "ob.filename IS NOT NULL";

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
        Db { conn, duration_akey }
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Albums with track counts, ordered for display.
    pub fn albums(&self) -> Result<Vec<Album>> {
        let mut st = self.conn.prepare(
            "SELECT al.id, al.value, \
                    (SELECT COUNT(*) FROM object_body ob \
                       WHERE ob.album_id = al.id AND ob.filename IS NOT NULL) \
             FROM albums al ORDER BY al.sort_str, al.value",
        )?;
        let rows = st.query_map([], |r| {
            Ok(Album { id: r.get(0)?, name: r.get(1)?, track_count: r.get(2)? })
        })?;
        rows.collect()
    }

    pub fn artists(&self) -> Result<Vec<Artist>> {
        let mut st = self
            .conn
            .prepare("SELECT id, value FROM artists ORDER BY sort_str, value")?;
        let rows = st.query_map([], |r| Ok(Artist { id: r.get(0)?, name: r.get(1)? }))?;
        rows.collect()
    }

    /// Tracks of one album, in disc/track order.
    pub fn album_tracks(&self, album_id: i64) -> Result<Vec<Track>> {
        self.query_tracks(
            &format!("WHERE {TRACK_WHERE} AND ob.album_id = ?1"),
            "ob.disc_no, ob.series_no, ob.child_index",
            [album_id],
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
        let v = self.query_tracks(
            &format!("WHERE {TRACK_WHERE} AND ob.filename = ?1"),
            "ob.object_id",
            [filename],
        )?;
        Ok(v.into_iter().next())
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
            Some(r) => Ok(Some(Art {
                bmpfile: r.get::<_, Option<String>>(0)?,
                source_path: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                data_offset: r.get(2)?,
                data_size: r.get(3)?,
                width: r.get(4)?,
                height: r.get(5)?,
            })),
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
                    COALESCE(ob.is_high_resolution,0), ob.othumb_id \
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
                filename: r.get::<_, Option<String>>(4)?.unwrap_or_default(),
                disc_no: r.get(5)?,
                track_no: r.get(6)?,
                duration_raw: r.get(7)?,
                is_hires: r.get::<_, i64>(8)? != 0,
                othumb_id: r.get(9)?,
            })
        })?;
        rows.collect()
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
            CREATE TABLE object_body (
                object_id INTEGER PRIMARY KEY AUTOINCREMENT, object_type INTEGER NOT NULL,
                child_index INTEGER, media_type INTEGER DEFAULT 0, format INTEGER DEFAULT 0,
                initial INTEGER, sort_str TEXT, search_str TEXT, title TEXT DEFAULT "",
                addedtime INTEGER DEFAULT 0, filename TEXT, filesize INTEGER,
                series_no INTEGER, disc_no INTEGER, is_high_resolution INTEGER,
                album_id INTEGER, artist_id INTEGER, othumb_id INTEGER, mthumb_id INTEGER);
            INSERT INTO albums  VALUES (10,0,'last smoke','last smoke','Last Smoke Before the Snowstorm');
            INSERT INTO albums  VALUES (11,0,'harvest','harvest','Harvest Moon');
            INSERT INTO artists VALUES (20,0,'leftwich','leftwich','Benjamin Francis Leftwich',NULL,0,0,0,0);
            INSERT INTO artists VALUES (21,0,'cold','cold','Cold Stone & Sea',NULL,0,0,0,0);
            INSERT INTO schema  VALUES (1,7,2,'DURATION');
            INSERT INTO images  VALUES (100,0,4096,20000,'/music/atlas.flac','d1','/db/thumb/100.bmp',92,92);
            INSERT INTO object_body (object_id,object_type,child_index,title,filename,series_no,disc_no,is_high_resolution,album_id,artist_id,othumb_id,addedtime)
              VALUES (1,1,0,'Atlas Hands','/music/atlas.flac',1,1,1,10,20,100,5000);
            INSERT INTO object_body (object_id,object_type,child_index,title,filename,series_no,disc_no,is_high_resolution,album_id,artist_id,othumb_id,addedtime)
              VALUES (2,1,1,'Box of Stones','/music/box.flac',2,1,1,10,20,NULL,5001);
            INSERT INTO object_body (object_id,object_type,child_index,title,filename,series_no,disc_no,is_high_resolution,album_id,artist_id,othumb_id,addedtime)
              VALUES (3,1,0,'Harvest Moon','/music/harvest.flac',1,1,0,11,21,NULL,4000);
            INSERT INTO object_body (object_id,object_type,title,filename,album_id) VALUES (9,0,'A Folder',NULL,NULL);
            INSERT INTO object_ext_int VALUES (1,7,272000);
            INSERT INTO object_ext_int VALUES (3,7,303000);
            "#,
        ).unwrap();
        Db::wrap(conn)
    }

    #[test]
    fn duration_akey_resolved() {
        assert_eq!(db().duration_akey, Some(7));
    }

    #[test]
    fn albums_with_counts() {
        let a = db().albums().unwrap();
        assert_eq!(a.len(), 2);
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

    #[test]
    fn now_playing_lookup_by_filename() {
        let t = db().track_by_filename("/music/harvest.flac").unwrap().unwrap();
        assert_eq!(t.title, "Harvest Moon");
        assert_eq!(t.album, "Harvest Moon");
        assert_eq!(t.duration_raw, Some(303000));
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
