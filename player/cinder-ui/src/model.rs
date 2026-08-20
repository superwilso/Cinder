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
    /// Hi-Res Audio, from the DB's own flag — the same one the Now Playing badge reads. A filter
    /// axis rather than a decoration: Sony has "Hi-Res only" and on the reference library it is
    /// the difference between 3,463 tracks and 1.
    pub is_hires: bool,
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

/// One directory in the FOLDER browse tree.
///
/// Sony has this and it is the only view that answers "where did this file actually come from" —
/// which on a player you fill over USB-MSC is a question that comes up constantly, especially for
/// anything the tag scanner filed under the wrong artist.
///
/// Flattened into `Library::folders` with index links rather than nested `Vec<FolderRow>` values:
/// the UI needs to walk UP as well as down, an owned tree cannot hold a parent pointer without
/// `Rc`, and every screen here is already an index into a flat list.
#[derive(Clone, Default)]
pub struct FolderRow {
    /// Absolute directory path (`/contents/Music/Artist/Album`). The row LABEL is `name`; this is
    /// what Track information shows and what makes two same-named folders distinguishable.
    pub path: String,
    /// Last path segment — what the row draws. For a storage root it is the mount point itself.
    pub name: String,
    /// Index into `Library::folders`, or `None` for a storage root.
    pub parent: Option<usize>,
    /// Child directories, already in display order.
    pub subdirs: Vec<usize>,
    /// Tracks sitting DIRECTLY in this directory, in filename order — which for a music folder is
    /// track order, and is the order the files are actually in on the volume.
    pub tracks: Vec<SongRow>,
    /// Tracks here AND in everything below. The row's count: a folder that holds only subfolders
    /// would otherwise read "0 tracks" while containing a whole discography.
    pub total: u32,
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
    /// True for a playlist CINDER owns — one made on the device (or dropped into
    /// `/contents/cinder_playlists` from a PC), stored as an `.m3u8` file. False for one read out
    /// of Sony's MediaStore database, which this app can browse and play but must not write: that
    /// database is rebuilt by a rescan and its object ids are re-issued with it.
    ///
    /// The flag decides whether the page draws its edit bar, so it is not decoration — a row that
    /// claimed to be editable and was not would offer controls that silently do nothing.
    pub user: bool,
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
    /// The other filter axis: show only Hi-Res tracks. Independent of the genre — the two AND
    /// together, so "Hi-Res jazz" is expressible and neither has to know about the other.
    pub filter_hires: bool,
    /// How many tracks carry the Hi-Res flag. Counted once at library build so the picker can
    /// label the row without sweeping every track each frame it is on screen.
    pub hires_tracks: u32,
    /// The FOLDER tree, flattened. Built once at library build; `folder_roots` are the entries
    /// with no parent (one per storage volume that holds music).
    pub folders: Vec<FolderRow>,
    pub folder_roots: Vec<usize>,
}

impl Library {
    /// Does this row survive the active filter? The single predicate every filtered list asks.
    pub fn passes(&self, s: &SongRow) -> bool {
        if self.filter_hires && !s.is_hires {
            return false;
        }
        match self.filter_genre {
            None => true,
            Some(id) => s.genre_id == id,
        }
    }
    /// Is anything filtering at all? Cheaper than asking each axis at every call site, and it is
    /// the question `visible_songs` and the strip actually want.
    pub fn filtered(&self) -> bool {
        self.filter_genre.is_some() || self.filter_hires
    }
    /// How many tracks are visible under the active filter. Sort-independent, so `row_count` can
    /// answer without knowing which sort chip is selected.
    pub fn visible_songs(&self) -> usize {
        if !self.filtered() {
            return self.songs.len();
        }
        self.songs.iter().filter(|s| self.passes(s)).count()
    }
    /// The active filter's display name, for captions and the strip. `None` when nothing is
    /// filtered; the two axes compose with a middot when both are on.
    pub fn filter_name(&self) -> Option<String> {
        let genre = self
            .filter_genre
            .and_then(|id| self.genres.iter().find(|g| g.id == id))
            .map(|g| g.name.clone());
        match (genre, self.filter_hires) {
            (Some(g), false) => Some(g),
            (Some(g), true) => Some(format!("{g} · Hi-Res")),
            (None, true) => Some("Hi-Res".to_string()),
            (None, false) => None,
        }
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
                // A couple of Hi-Res rows so the host preview's Hi-Res filter is exercisable.
                is_hires: i % 5 == 0,
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
                // Sample data stands in for Sony's database rows on the host preview.
                user: false,
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
        let hires_tracks = songs.iter().filter(|s| s.is_hires).count() as u32;
        // The sample data has no paths behind it, so the host preview's folder tree is a small
        // hand-built one rather than a derived one — enough to exercise both row kinds.
        let folders = vec![
            FolderRow {
                path: "/contents/Music".into(), name: "/contents/Music".into(),
                parent: None, subdirs: vec![1, 2], tracks: Vec::new(),
                total: songs.len() as u32,
            },
            FolderRow {
                path: "/contents/Music/Hollow Pines".into(), name: "Hollow Pines".into(),
                parent: Some(0), subdirs: Vec::new(),
                tracks: songs.iter().take(4).cloned().collect(), total: 4,
            },
            FolderRow {
                path: "/contents/Music/Vesper Lane".into(), name: "Vesper Lane".into(),
                parent: Some(0), subdirs: Vec::new(),
                tracks: songs.iter().skip(4).cloned().collect(),
                total: songs.len().saturating_sub(4) as u32,
            },
        ];
        Library {
            songs,
            album_groups,
            artists,
            playlists,
            thumbs: Default::default(),
            genres,
            filter_genre: None,
            filter_hires: false,
            hires_tracks,
            folders,
            folder_roots: vec![0],
        }
    }
}
