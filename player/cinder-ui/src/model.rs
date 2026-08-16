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
    /// `genres.id` for the genre FILTER. An id, not a name: 3,463 tracks share 95 genres on the
    /// reference device, so a per-row string would be thousands of heap allocations to say one of
    /// 95 things. 0 = not resolved (host/sample data). Names come from `Library::genres`.
    pub genre_id: i64,
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

/// One entry in the genre filter picker: the id to filter by, its display name, and how many
/// tracks carry it — the count is what makes the picker useful rather than a wall of 95 words.
#[derive(Clone)]
pub struct GenreRow {
    pub id: i64,
    /// Already substituted for display: the DB's empty genre becomes "(No genre)". Confirmed real
    /// on the reference device — 482 of 3,463 tracks point at a genre row whose value is "".
    pub name: String,
    pub tracks: u32,
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
    /// Gradient seeds — the fallback when a cover has not been decoded yet.
    pub arts: Vec<String>,
    /// DB album ids behind `arts`, in the same order. The Artists tab used to draw only the
    /// gradients, which is why its rows looked wrong next to the Albums tab: covers live in
    /// `Library::thumbs` keyed by album id, and without the id there was nothing to look them up
    /// with. Empty for the host/sim sample data, which has no DB.
    pub album_ids: Vec<i64>,
}

/// One playlist row (Playlists tab). `id` is the DB container's `object_id` — the handle
/// `cinder-db::playlist_tracks` takes to resolve the member tracks in saved order. It is 0 for
/// the host/sim sample data, which has no DB behind it.
#[derive(Clone)]
pub struct PlaylistRow {
    pub id: i64,
    pub name: String,
    pub tracks: u32,
    pub art: String,
    /// Members in the user's saved order. Resolved once at library build, like `AlbumRow`'s, so
    /// the drill-in page is a pure view and needs no DB access per frame. `tracks` is the DB's own
    /// count and can legitimately exceed this when a member file no longer resolves.
    pub track_list: Vec<SongRow>,
}

/// The whole browsable library, as owned rows. Built once (from the DB on device, or the
/// sample constants on host) and held by `nav::App`.
#[derive(Clone, Default)]
pub struct Library {
    pub songs: Vec<SongRow>,
    pub album_groups: Vec<ArtistGroup>,
    pub artists: Vec<ArtistRow>,
    pub playlists: Vec<PlaylistRow>,
    /// Real cover thumbnails, keyed by `album_id`, pre-scaled to the 48x48 the list rows draw.
    ///
    /// Populated by the shell (cinder-ffi) from its on-disk art cache; empty on host/sample data
    /// and empty for any album whose cover hasn't been decoded yet. Rows fall back to the
    /// generated gradient when a key is missing, so this can fill in progressively while the
    /// background builder works through the library. Rendering stays pure: it only ever reads.
    ///
    /// Kept pre-scaled because decoding is what's expensive — 365 ms for one of these covers on
    /// device (they're 1425x1425 JPEGs embedded in the FLACs), which is why they cannot be
    /// resolved during a scroll.
    pub thumbs: std::collections::HashMap<i64, crate::art::Image>,
    /// Every genre that at least one track carries, with its count. Built once at library build.
    pub genres: Vec<GenreRow>,
    /// The ACTIVE genre filter: `Some(id)` hides every track that does not carry it.
    ///
    /// It lives on the LIBRARY rather than on `App` on purpose. `song_order`, `row_count`,
    /// `albums_build`, `az_present` and `content_h` all already take `&Library`, so putting it here
    /// means every one of them honours the filter with no signature change. Threading a filter
    /// argument through ten call sites is precisely how a hit test drifts out of step with a
    /// render, which is the bug this file's neighbours keep getting bitten by.
    pub filter_genre: Option<i64>,
}

impl Library {
    /// Does this row survive the active filter? The single predicate every filtered list asks.
    pub fn passes(&self, s: &SongRow) -> bool {
        match self.filter_genre {
            None => true,
            Some(id) => s.genre_id == id,
        }
    }
    /// How many tracks are visible under the active filter. Sort-independent, so `row_count` can
    /// answer without knowing which sort chip is selected.
    pub fn visible_songs(&self) -> usize {
        match self.filter_genre {
            None => self.songs.len(),
            Some(_) => self.songs.iter().filter(|s| self.passes(s)).count(),
        }
    }
    /// The active filter's display name, for captions. `None` when nothing is filtered.
    pub fn filter_name(&self) -> Option<&str> {
        let id = self.filter_genre?;
        self.genres.iter().find(|g| g.id == id).map(|g| g.name.as_str())
    }

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
        let songs: Vec<SongRow> = data::SONGS
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
                // Two sample genres so the host preview's filter has something to do.
                genre_id: (i % 2) as i64 + 1,
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
                // Sample data has no DB behind it, so there are no ids to look covers up by —
                // the gradient fallback is the correct and only answer here.
                album_ids: Vec::new(),
            })
            .collect();
        // Sample members: the first `k` songs, cycled. The device builds these from the DB — this
        // only has to give the drill-in page something real to lay out in the host preview.
        let playlists = data::PLAYLISTS
            .iter()
            .enumerate()
            .map(|(pi, p)| PlaylistRow {
                id: pi as i64 + 1,
                name: p.n.into(),
                tracks: p.k,
                art: p.art.into(),
                track_list: (0..p.k as usize)
                    .filter_map(|i| songs.get((i + pi) % songs.len().max(1)).cloned())
                    .collect(),
            })
            .collect();
        // Sample genres matching the ids the sample songs carry, so the host preview's picker and
        // filter are exercisable without a device.
        let genres = vec![
            GenreRow { id: 1, name: "Alternative".into(), tracks: songs.iter().filter(|s| s.genre_id == 1).count() as u32 },
            GenreRow { id: 2, name: "Electronic".into(), tracks: songs.iter().filter(|s| s.genre_id == 2).count() as u32 },
        ];
        Library {
            songs,
            album_groups,
            artists,
            playlists,
            thumbs: Default::default(),
            genres,
            filter_genre: None,
        }
    }
}
