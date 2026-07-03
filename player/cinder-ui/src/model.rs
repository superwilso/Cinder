//! Owned library view-model — the bridge between the library database (cinder-db, populated
//! by cinder-ffi on the device) and the pure-UI render functions. cinder-ui stays DB-free:
//! it only ever sees these owned rows, so host PNG preview and the device share one render
//! path. `Library::sample()` rebuilds the design demo data so the host preview is unchanged.

use crate::data;

/// One song row (Songs tab, queue, artist top-songs). `object_id` is the library key the
/// shell uses to actually start playback (Action::PlayIndex resolves through it). The trailing
/// fields are SORT KEYS for the Songs-tab SORT chip (album order / recently added / release
/// year) — they don't render, they only order. All default to 0 for host/sample data.
#[derive(Clone, Default)]
pub struct SongRow {
    pub title: String,
    pub artist: String,
    pub dur: String,
    pub art: String,
    pub object_id: i64,
    /// Sort keys (populated from the DB by the shell; 0 = unknown).
    pub album_id: i64,
    pub disc: i32,
    pub track: i32,
    pub added: i64, // addedtime — "recently added" sort
    pub year: i32,  // release year — "release year" sort (0 = unresolved)
}

/// One album row (Albums tab, grouped under its artist).
#[derive(Clone, Default)]
pub struct AlbumRow {
    pub name: String,
    pub artist: String,
    pub year: String,
    pub tracks: u32,
    pub art: String,
    pub album_id: i64,
    /// "Recently added" sort key: the newest addedtime across the album's tracks (0 = unknown).
    pub added: i64,
    /// The album's tracks in play order (for the drill-in view). May be empty if not loaded.
    pub track_list: Vec<SongRow>,
}

/// Albums grouped by artist (Albums tab section structure).
#[derive(Clone)]
pub struct ArtistGroup {
    pub artist: String,
    pub albums: Vec<AlbumRow>,
}

/// One artist row (Artists tab); `arts` are 1–2 art keys for the overlapping stack.
#[derive(Clone)]
pub struct ArtistRow {
    pub name: String,
    pub albums: u32,
    pub tracks: u32,
    pub arts: Vec<String>,
}

/// One playlist row (Playlists tab).
#[derive(Clone)]
pub struct PlaylistRow {
    pub name: String,
    pub tracks: u32,
    pub art: String,
}

/// The whole browsable library, as owned rows. Built once (from the DB on device, or the
/// sample constants on host) and held by `nav::App`.
#[derive(Clone, Default)]
pub struct Library {
    pub songs: Vec<SongRow>,
    pub album_groups: Vec<ArtistGroup>,
    pub artists: Vec<ArtistRow>,
    pub playlists: Vec<PlaylistRow>,
}

impl Library {
    /// Total album count across all groups (for the header caption).
    pub fn album_count(&self) -> usize {
        self.album_groups.iter().map(|g| g.albums.len()).sum()
    }

    /// Flat list of albums in display order (Albums tab cursor indexes into this).
    pub fn albums_flat(&self) -> Vec<&AlbumRow> {
        self.album_groups.iter().flat_map(|g| g.albums.iter()).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.songs.is_empty()
            && self.album_groups.is_empty()
            && self.artists.is_empty()
            && self.playlists.is_empty()
    }

    /// The design demo data (mirrors `data::` constants) so host preview is unchanged.
    pub fn sample() -> Self {
        let songs = data::SONGS
            .iter()
            .enumerate()
            .map(|(i, s)| SongRow {
                title: s.t.into(),
                artist: s.a.into(),
                dur: s.d.into(),
                art: s.art.into(),
                object_id: i as i64,
                // sample sort keys so the host preview's SORT chip visibly reorders
                album_id: (i / 3) as i64,
                disc: 1,
                track: (i % 3) as i32 + 1,
                added: 10_000 - i as i64,
                year: 1990 + (i as i32 * 3 % 30),
            })
            .collect();
        let album_groups = data::ALBUM_GROUPS
            .iter()
            .map(|g| ArtistGroup {
                artist: g.artist.into(),
                albums: g
                    .albums
                    .iter()
                    .map(|a| {
                        let year_num: i32 = a.y.parse().unwrap_or(0);
                        AlbumRow {
                            name: a.n.into(),
                            artist: g.artist.into(),
                            year: a.y.into(),
                            tracks: a.k,
                            art: a.art.into(),
                            album_id: 0,
                            added: year_num as i64, // sample: newer albums sort first
                            // sample track list: synthesize a few rows so the drill-in preview works
                            track_list: (0..a.k.min(8))
                                .map(|i| SongRow {
                                    title: format!("{} — Track {}", a.n, i + 1),
                                    artist: g.artist.into(),
                                    dur: format!("{}:{:02}", 3 + (i % 3), (i * 17) % 60),
                                    art: a.art.into(),
                                    object_id: i as i64,
                                    disc: 1,
                                    track: i as i32 + 1,
                                    year: year_num,
                                    ..Default::default()
                                })
                                .collect(),
                        }
                    })
                    .collect(),
            })
            .collect();
        let artists = data::ARTISTS
            .iter()
            .map(|a| ArtistRow {
                name: a.n.into(),
                albums: a.al,
                tracks: a.tr,
                arts: a.arts.iter().map(|s| s.to_string()).collect(),
            })
            .collect();
        let playlists = data::PLAYLISTS
            .iter()
            .map(|p| PlaylistRow { name: p.n.into(), tracks: p.k, art: p.art.into() })
            .collect();
        Library { songs, album_groups, artists, playlists }
    }
}
