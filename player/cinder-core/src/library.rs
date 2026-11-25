//! The library, built from Sony's MTPDB or Cinder's own index: songs, albums, artists, playlists,
//! folders, SensMe channels, audiobooks and storage. Moved unchanged from cinder-ffi so v0.4 and
//! v0.5 build the same `cinder_ui::Library` (docs/PLAN_v0.5.md, P3).

use crate::{books, cue, series, shared, stats};

/// Build the browsable `Library` view-model from the library DB in a single pass over the
/// tracks (+ the album list for accurate counts/order). Albums are grouped by artist; the
/// Songs tab gets every track; artists are derived with album/track counts. Playlists aren't
/// in cinder-db yet, so that tab is empty for now. Art keys use the album/title so each item
/// gets a distinct hashed gradient until real thumbnails are decoded.
/// Stand-in album id for a row that has no album behind it. Deliberately not 0 — 0 is a perfectly
/// valid SQLite primary key, and using it would make an artless row collide with a real album's
/// cached cover.
pub const NO_ALBUM_ID: i64 = i64::MIN;

/// The Storage screen's numbers: the tracks just built plus what Sony's scanner recorded about
/// each file, and the SD card's size from statvfs. One query, at library build.
pub fn storage_summary(
    db: &cinder_db::Db,
    tracks: &[cinder_db::Track],
) -> cinder_ui::storage::Summary {
    let t0 = std::time::Instant::now();
    let facts: std::collections::HashMap<i64, cinder_db::FileFacts> = db
        .file_facts()
        .unwrap_or_else(|e| {
            eprintln!(
                "cinder-ffi: storage: file facts unreadable ({e}) — sizes and quality read 0"
            );
            Vec::new()
        })
        .into_iter()
        .map(|f| (f.object_id, f))
        .collect();
    let files: Vec<cinder_ui::storage::TrackFile> = tracks
        .iter()
        .map(|t| {
            let f = facts.get(&t.object_id).copied().unwrap_or_default();
            cinder_ui::storage::TrackFile {
                path: &t.filename,
                title: &t.title,
                album_id: t.album_id.unwrap_or(0),
                album: &t.album,
                ms: t.duration_raw.unwrap_or(0).max(0) as u64,
                added: t.added,
                bytes: f.bytes,
                sample_rate: f.sample_rate,
                bits: f.bits,
                bitrate: f.bitrate,
                hires: t.is_hires,
            }
        })
        .collect();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let (sd_total, sd_free) = statvfs_mb("/contents_ext");
    let s = cinder_ui::storage::summarise(&files, now, sd_total, sd_free);
    eprintln!(
        "cinder-ffi: storage: {} tracks, {} bytes, {} formats, SD {sd_total} MB ({} ms)",
        s.tracks,
        s.bytes,
        s.formats.len(),
        t0.elapsed().as_millis()
    );
    s
}

/// (total, free) MB of the filesystem at `path`, or (0, 0) when nothing is mounted there. A path
/// that is only a directory on /data would report /data's size, so the card counts only when its
/// filesystem differs from the root's.
pub fn statvfs_mb(path: &str) -> (i32, i32) {
    fn read(p: &str) -> Option<libc::statvfs> {
        let c = std::ffi::CString::new(p).ok()?;
        let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
        (unsafe { libc::statvfs(c.as_ptr(), &mut st) } == 0).then_some(st)
    }
    let (Some(st), Some(root)) = (read(path), read("/")) else {
        return (0, 0);
    };
    if st.f_fsid == root.f_fsid && st.f_blocks == root.f_blocks {
        return (0, 0);
    }
    let mb = |blocks: u64| (blocks as u128 * st.f_frsize as u128 / 1_048_576) as i32;
    // f_blocks is u32 on the player (32-bit ARM) and u64 on a PC: the cast is only a no-op here.
    #[allow(clippy::unnecessary_cast)]
    let (blocks, avail) = (st.f_blocks as u64, st.f_bavail as u64);
    (mb(blocks), mb(avail))
}

/// Cinder's own library index (docs/PLAN_hardware_first.md, Phase 2). On /db: ext4 (a first scan is
/// 43 s there against 91 s on /contents' FAT), never handed to the PC, ~80 MB free; /data is 35 MB
/// and an install needs ~8.5 MB of it. It was on /contents for a night; `index_move` brings it over.
pub const INDEX_PATH: &str = "/db/cinder/library.db";
pub const INDEX_OLD: &str = "/contents/.cinder/library.db";

/// Bring the index over from its first home (copy, then delete the old file and its folder).
pub fn index_move() {
    let _ = std::fs::create_dir_all("/db/cinder");
    if std::path::Path::new(INDEX_OLD).exists()
        && !std::path::Path::new(INDEX_PATH).exists()
        && std::fs::copy(INDEX_OLD, INDEX_PATH).is_ok()
    {
        let _ = std::fs::remove_file(INDEX_OLD);
        let _ = std::fs::remove_dir("/contents/.cinder");
        eprintln!("cinder-ffi: index: moved to {INDEX_PATH}");
    }
}
pub const INDEX_ROOTS: [&str; 2] = ["/contents/MUSIC", "/contents_ext/MUSIC"];
/// `own_index` in cinder_advanced.conf: the Library's songs come from the index.
pub static OWN_INDEX: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// A scan added, changed or removed something: the shell rebuilds the Library (own_index only).
pub static INDEX_CHANGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// A scan is running: a second request while it does is dropped, not queued.
pub static INDEX_BUSY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 1 once after a scan that changed the index (and `own_index` is on); the shell then reloads.
#[no_mangle]
pub extern "C" fn cinder_index_take_changed() -> libc::c_int {
    INDEX_CHANGED.swap(false, std::sync::atomic::Ordering::Relaxed) as libc::c_int
}

#[no_mangle]
pub extern "C" fn cinder_set_own_index(on: libc::c_int) {
    OWN_INDEX.store(on != 0, std::sync::atomic::Ordering::Relaxed);
}

/// With `own_index` on and an index on the drive, the Library's track list comes from it
/// ([`cinder_db::index::tracks`]: MTPDB's rows, the index's tags). Otherwise, or if the index is
/// missing, empty or unreadable, MTPDB's list as it was.
pub fn index_tracks(mtpdb: Vec<cinder_db::Track>) -> Vec<cinder_db::Track> {
    if OWN_INDEX.load(std::sync::atomic::Ordering::Relaxed) {
        index_move();
    }
    if !OWN_INDEX.load(std::sync::atomic::Ordering::Relaxed)
        || !std::path::Path::new(INDEX_PATH).exists()
    {
        return mtpdb;
    }
    let n = mtpdb.len();
    match cinder_db::index::open(INDEX_PATH)
        .and_then(|c| cinder_db::index::tracks(&c, mtpdb.clone()))
    {
        Ok(v) if !v.is_empty() => {
            eprintln!(
                "cinder-ffi: library from Cinder's index: {} songs (MTPDB {n})",
                v.len()
            );
            v
        }
        Ok(_) => mtpdb,
        Err(e) => {
            eprintln!("cinder-ffi: index unreadable ({e}) — library from MTPDB");
            mtpdb
        }
    }
}

/// [`build_library`], with the stats store read against the tracks while their paths are in hand
/// — the store is keyed by path and the UI by object id, and this is the one place both exist.
pub fn build_library_with(
    db: &cinder_db::Db,
    stats: Option<&stats::Store>,
    audiobooks: bool,
) -> cinder_ui::Library {
    use cinder_ui::model::{AlbumRow, ArtistGroup, ArtistRow, SongRow};
    use std::collections::BTreeMap;

    // Resolve release-year FKs once (best-effort; empty map if the table shape differs — years
    // then stay blank, exactly as before). Shared by the song-row builder + the album year label.
    // PER-PHASE TIMING, ONCE PER OPEN. The whole of this function is the ~4.5 s of boot dead time
    // (device 2026-09-05: cinder_db_open at t=2.716, "restore playback context" at t=7.271), and
    // until now nobody had a device-side breakdown of it — only a host profile, which says roughly
    // 77% SQLite and disagrees with the device by about 100x overall. A 1 GHz in-order A7 against
    // that host should be nearer 20-30x for this kind of work, so the gap itself is the finding.
    // One line per library open, which is once per boot.
    let t_total = std::time::Instant::now();
    let t_phase = std::time::Instant::now();
    let years = db.release_years();
    let ms_years = t_phase.elapsed().as_millis();
    let year_num = |id: Option<i64>| -> i32 {
        id.and_then(|i| years.get(&i))
            .and_then(|s| s.trim().parse::<i32>().ok())
            .unwrap_or(0)
    };

    // Only the release YEAR needs the FK map; everything else is on the track itself, so the
    // shared builder covers it and this closure just fills the one field in.
    let song_row = |t: &cinder_db::Track| SongRow {
        year: year_num(t.releaseyear_id),
        ..song_row_of(t)
    };

    let t_phase = std::time::Instant::now();
    // A FAILED QUERY SAYS SO. These three used to be `unwrap_or_default()`, which turned any error
    // into an empty list and logged nothing — so a library that would not read looked exactly like a
    // card with no music on it (reported 2026-09-13: `library loaded — 0 tracks`, no reason given).
    let all_tracks = db.tracks(cinder_db::Sort::Title).unwrap_or_else(|e| {
        eprintln!("cinder-ffi: track query FAILED — {e} (the library will show no songs)");
        Vec::new()
    });
    let all_tracks = index_tracks(all_tracks);
    // AUDIOBOOKS ARE NOT MUSIC. With the component on, everything under `LEARNING` goes to the
    // Audiobooks screen and is kept out of Songs, Albums, Artists, Folders and every shuffle built
    // from them. With it off (the installer's choice, `audiobooks_enabled`) nothing is set apart
    // and those files are music, as they were before the component existed.
    let (book_tracks, tracks): (Vec<cinder_db::Track>, Vec<cinder_db::Track>) = all_tracks
        .into_iter()
        .partition(|t| audiobooks && books::is_audiobook(&t.filename));
    // SONGS FLINT STORED ONCE for two albums (`shared.rs`): each second entry joins the library
    // right after the file it plays, under its own album. Not in Folders: there is one file.
    let shared_extra = shared::tracks(&shared::read_all(), &tracks);
    let (tracks, is_shared): (Vec<cinder_db::Track>, Vec<bool>) = if shared_extra.is_empty() {
        let n = tracks.len();
        (tracks, vec![false; n])
    } else {
        eprintln!(
            "cinder-ffi: {} song(s) stored once, listed under a second album",
            shared_extra.len()
        );
        let mut by_kept: BTreeMap<i64, Vec<cinder_db::Track>> = BTreeMap::new();
        for t in shared_extra.iter().cloned() {
            by_kept.entry(t.object_id).or_default().push(t);
        }
        let mut out = Vec::with_capacity(tracks.len() + shared_extra.len());
        for t in tracks {
            let extra = by_kept.remove(&t.object_id).unwrap_or_default();
            out.push((t, false));
            out.extend(extra.into_iter().map(|e| (e, true)));
        }
        out.into_iter().unzip()
    };
    let books = books::rows(&book_tracks, song_row_of, series::read_file);
    if !books.is_empty() {
        eprintln!(
            "cinder-ffi: audiobooks: {} books, {} chapters",
            books.len(),
            book_tracks.len()
        );
    }
    let ms_tracks = t_phase.elapsed().as_millis();
    let track_stats = stats
        .map(|s| s.by_object_id(tracks.iter().map(|t| (t.object_id, t.filename.as_str()))))
        .unwrap_or_default();
    // WHERE THE MUSIC IS, one line per open. "The music on my SD card doesn't show up" can only be
    // answered from a log if the log says how much of the library each storage holds: a card whose
    // rows are missing from Sony's store reads `SD 0` here, and rows whose storage root did not map
    // onto either mount read as unresolved. Reported 2026-09-13 with nothing in the log to say which.
    let (mut on_internal, mut on_sd, mut unresolved) = (0usize, 0usize, 0usize);
    for t in &tracks {
        let f = t.filename.as_str();
        if f.starts_with("/contents_ext/") || f.starts_with("/data/mnt/external/") {
            on_sd += 1;
        } else if f.starts_with("/contents/") || f.starts_with("/data/mnt/internal/") {
            on_internal += 1;
        } else {
            unresolved += 1;
        }
    }
    eprintln!("cinder-ffi: library by storage — internal {on_internal}, SD {on_sd}, unresolved {unresolved}");
    let mut album_artist: BTreeMap<i64, String> = BTreeMap::new();
    // Per artist: their distinct albums as name → album id, plus a track count.
    //
    // The name and the id have to live in ONE ordered structure. They used to be a sorted
    // `BTreeSet<String>` of names beside an insertion-ordered `Vec<i64>` of ids, with a comment
    // claiming the two lined up — they did not, and could not: one was alphabetical and the other
    // was DB row order. The Artists tab draws the cover by id and the gradient by name, so 78 rows
    // on the test library showed one album's artwork labelled with another album's colours.
    let mut artist_albums: BTreeMap<String, (BTreeMap<String, i64>, u32)> = BTreeMap::new();
    // SENSME, one query for the whole library. Empty on a library nobody has analysed, which is
    // every library until a PC tool (Flint, or Sony's Music Center) writes the tags — the map is
    // what the PLAYER'S scanner made of whichever tag it found, so both tools land here identically.
    let t_phase = std::time::Instant::now();
    let sensme = db.sensme();
    let ms_sensme = t_phase.elapsed().as_millis();
    let mut songs = Vec::with_capacity(tracks.len());
    // ALBUM ARTIST is what browsing groups by — it is the default, and the track artist is only a
    // fallback for files that carry no album artist at all. Grouping by the track artist shatters
    // compilations: on this library 24 albums span several track artists and one DJ mix spans 26,
    // so it showed up as 26 one-track albums under 26 different people. By album artist, ZERO
    // albums split. (The Songs tab still shows the TRACK artist — that is the right thing on a
    // song row, and it is where a featured guest belongs.)
    //
    // The old code also took the album's artist from whichever track sorted FIRST BY TITLE, which
    // for a compilation is simply an arbitrary pick.
    let group_artist = |t: &cinder_db::Track| -> String {
        if t.album_artist.trim().is_empty() {
            t.artist.clone()
        } else {
            t.album_artist.clone()
        }
    };
    for t in &tracks {
        if let Some(aid) = t.album_id {
            album_artist.entry(aid).or_insert_with(|| group_artist(t));
        }
        songs.push(cinder_ui::model::SongRow {
            sensme: sensme.get(&t.object_id).map_or(0, |s| s.channels),
            ..song_row(t)
        });
        let e = artist_albums.entry(group_artist(t)).or_default();
        if !t.album.is_empty() {
            // Keyed by name (that is what the album COUNT has always meant here). The id is
            // whichever track first supplies one — a track with no album_id leaves the slot at
            // NO_ALBUM_ID, which simply misses the thumbnail cache and draws the gradient.
            let slot = e.0.entry(t.album.clone()).or_insert(NO_ALBUM_ID);
            if *slot == NO_ALBUM_ID {
                if let Some(aid) = t.album_id {
                    *slot = aid;
                }
            }
        }
        e.1 += 1;
    }

    // Per-album track lists keyed by album ID (names can collide across artists), in the DB's
    // disc/track order — one query, grouped in a single pass. This is what the Album screen
    // shows under the header. Alongside, derive each album's "recently added" key (newest track
    // addedtime) and its release-year label (first track that resolves one) for the ORDER chip.
    let mut album_tracks: BTreeMap<i64, Vec<SongRow>> = BTreeMap::new();
    // The second entries go to their albums too, in their place by disc and track.
    let mut shared_albums: std::collections::HashSet<i64> = std::collections::HashSet::new();
    for t in &shared_extra {
        if let Some(aid) = t.album_id {
            album_tracks.entry(aid).or_default().push(song_row(t));
            shared_albums.insert(aid);
        }
    }
    let mut long_files: BTreeMap<i64, (String, i64)> = BTreeMap::new();
    let mut album_added: BTreeMap<i64, i64> = BTreeMap::new();
    let mut album_year: BTreeMap<i64, String> = BTreeMap::new();
    let t_phase = std::time::Instant::now();
    let album_order_rows = db.tracks_album_order().unwrap_or_else(|e| {
        eprintln!("cinder-ffi: album-order track query FAILED — {e} (albums will list no songs)");
        Vec::new()
    });
    let ms_album_order = t_phase.elapsed().as_millis();
    let t_phase = std::time::Instant::now();
    for t in album_order_rows
        .into_iter()
        .filter(|t| !(audiobooks && books::is_audiobook(&t.filename)))
    {
        if let Some(aid) = t.album_id {
            let a = album_added.entry(aid).or_insert(0);
            if t.added > *a {
                *a = t.added;
            }
            if let std::collections::btree_map::Entry::Vacant(e) = album_year.entry(aid) {
                if let Some(y) = t.releaseyear_id.and_then(|i| years.get(&i)) {
                    // Sony writes an unknown year as "0", not as nothing, and it printed as
                    // "0 · 10 tracks" under the album (seen on the owner's library, 2026-09-23).
                    if is_real_year(y) {
                        e.insert(y.clone());
                    }
                }
            }
            album_tracks.entry(aid).or_default().push(song_row(&t));
            // A file longer than 20 minutes may be a whole album with a CUE sheet beside it.
            // Only those are looked at: a few stats, not one per track in the library.
            if t.duration_raw.unwrap_or(0) > 20 * 60 * 1000 {
                long_files.insert(
                    t.object_id,
                    (t.filename.clone(), t.duration_raw.unwrap_or(0)),
                );
            }
        }
    }

    for aid in &shared_albums {
        if let Some(list) = album_tracks.get_mut(aid) {
            list.sort_by_key(|s| (s.disc, s.track));
        }
    }

    // Album list (ordered, with track counts) → rows, grouped by artist.
    let ms_album_group = t_phase.elapsed().as_millis();
    let t_phase = std::time::Instant::now();
    let album_list = db.albums().unwrap_or_else(|e| {
        eprintln!("cinder-ffi: album query FAILED — {e} (the Albums tab will be empty)");
        Vec::new()
    });
    let ms_albums = t_phase.elapsed().as_millis();
    // An album that holds only audiobook chapters is a book, not an album. (One that mixes the two
    // stays, with its music tracks.)
    let book_only_albums: std::collections::HashSet<i64> = {
        let music: std::collections::HashSet<i64> =
            tracks.iter().filter_map(|t| t.album_id).collect();
        book_tracks
            .iter()
            .filter_map(|t| t.album_id)
            .filter(|id| !music.contains(id))
            .collect()
    };
    let mut album_rows: Vec<AlbumRow> = album_list
        .into_iter()
        .filter(|a| !book_only_albums.contains(&a.id))
        .map(|a| {
            let trs = album_tracks.remove(&a.id).unwrap_or_default();
            let mut artist = album_artist.get(&a.id).cloned().unwrap_or_default();
            if artist.is_empty() && !trs.is_empty() {
                artist = trs[0].artist.clone();
            }
            AlbumRow {
                cue: Vec::new(),
                artist,
                year: album_year.get(&a.id).cloned().unwrap_or_default(),
                // The DB's count, plus any second entries Flint listed here (`shared.rs`).
                tracks: (a.track_count.max(0) as u32).max(trs.len() as u32),
                art: a.name.clone(),
                added: album_added.get(&a.id).copied().unwrap_or(0),
                track_list: trs,
                name: a.name,
                album_id: a.id,
            }
        })
        .collect();

    // Include any albums that had tracks but were not returned by db.albums()
    for (aid, trs) in album_tracks {
        if trs.is_empty() {
            continue;
        }
        let name = trs[0].art.clone();
        if name.is_empty() {
            continue;
        }
        let artist = album_artist
            .get(&aid)
            .cloned()
            .unwrap_or_else(|| trs[0].artist.clone());
        let year = album_year.get(&aid).cloned().unwrap_or_default();
        let added = album_added.get(&aid).copied().unwrap_or(0);
        let count = trs.len() as u32;
        album_rows.push(AlbumRow {
            cue: Vec::new(),
            artist,
            year,
            tracks: count,
            art: name.clone(),
            added,
            track_list: trs,
            name,
            album_id: aid,
        });
    }

    // CUE sheets: an album whose long files have one lists the sheet's tracks on its page.
    let mut cue_albums = 0;
    for al in album_rows.iter_mut() {
        al.cue = cue_rows(&al.track_list, &long_files);
        cue_albums += usize::from(!al.cue.is_empty());
    }
    if cue_albums > 0 {
        eprintln!("cinder-ffi: CUE sheets — {cue_albums} album(s) listed by their CUE tracks");
    }

    // One alphabetical order for the whole Library (cinder_ui::collate): byte order put every
    // lowercase or accented name after "Z" — `alt‐J`, `bôa`, `the north` below `Zola Jesus`.
    album_rows.sort_by(|x, y| {
        cinder_ui::collate::cmp(&x.artist, &y.artist)
            .then_with(|| cinder_ui::collate::cmp(&x.name, &y.name))
    });
    let mut album_groups: Vec<ArtistGroup> = Vec::new();
    for ar in album_rows {
        match album_groups.last_mut() {
            Some(g) if g.artist == ar.artist => g.albums.push(ar),
            _ => album_groups.push(ArtistGroup {
                artist: ar.artist.clone(),
                albums: vec![ar],
            }),
        }
    }

    let mut artists: Vec<ArtistRow> = artist_albums
        .into_iter()
        .filter(|(n, _)| !n.is_empty())
        .map(|(name, (albs, tr))| {
            // The cover stack draws at most two. Both arrays are taken from the SAME iterator in
            // the SAME order, so `arts[i]` and `album_ids[i]` are guaranteed to be one album.
            let (arts, album_ids): (Vec<String>, Vec<i64>) = if albs.is_empty() {
                (vec![name.clone()], vec![NO_ALBUM_ID])
            } else {
                albs.iter().take(2).map(|(n, id)| (n.clone(), *id)).unzip()
            };
            ArtistRow {
                albums: albs.len() as u32,
                tracks: tr,
                arts,
                album_ids,
                name,
            }
        })
        .collect();
    artists.sort_by(|a, b| cinder_ui::collate::cmp(&a.name, &b.name));

    // Playlists: real ones from the DB (Sony keeps them as containers in a second object tree —
    // see Db::playlists). Empty-but-honest if the DB has none, rather than sample data.
    let playlists = db
        .playlists()
        .unwrap_or_default()
        .into_iter()
        .map(|p| cinder_ui::model::PlaylistRow {
            glued: Vec::new(),
            id: p.id,
            // From Sony's database: browsable and playable, but not ours to edit.
            user: false,
            // …and therefore never a CHOSEN cover: there is no file to write it in. These still
            // get the automatic one, off their first member track, like everything else.
            cover_custom: false,
            smart: false,
            rules: String::new(),
            edited: false,
            cover_album_id: 0, // the automatic cover, filled by `auto_playlist_covers` below
            name: p.name.clone(),
            tracks: p.track_count.max(0) as u32,
            // Members in the saved order, resolved once here so the drill-in page never touches
            // the DB. One query per playlist, and there are a handful of them — the same shape as
            // the per-album track lists above, which cost one query for the entire library.
            track_list: db
                .playlist_tracks(p.id)
                .unwrap_or_default()
                .iter()
                .map(&song_row)
                .collect(),
            // No cover of its own: hash the name so each playlist still gets distinct art,
            // the same fallback the album rows use.
            art: p.name,
        })
        .collect();

    // GENRES for the filter picker. Counted from the tracks we actually built rather than from the
    // genres table, so the list only ever offers something that will match at least one row — 101
    // genres exist on the reference device but only 95 are carried by a track.
    //
    // The empty genre is REAL and is the largest bucket (482 of 3,463), so it is kept and labelled
    // rather than dropped; hiding it would mean the filter silently could not reach an eighth of
    // the library. Sorted by count, biggest first: with 95 entries the useful ones have to be at
    // the top, and alphabetical would bury Rock behind Acid Jazz.
    let genres = {
        let names = db.genres();
        let mut counts: BTreeMap<i64, u32> = BTreeMap::new();
        for s in &songs {
            if s.genre_id != 0 {
                *counts.entry(s.genre_id).or_insert(0) += 1;
            }
        }
        let mut v: Vec<cinder_ui::model::GenreRow> = counts
            .into_iter()
            .map(|(id, tracks)| {
                let raw = names.get(&id).cloned().unwrap_or_default();
                let name = if raw.trim().is_empty() {
                    "(No genre)".to_string()
                } else {
                    raw
                };
                cinder_ui::model::GenreRow { id, name, tracks }
            })
            .collect();
        v.sort_by(|a, b| b.tracks.cmp(&a.tracks).then_with(|| a.name.cmp(&b.name)));
        eprintln!("cinder-ffi: genres: {} in use", v.len());
        v
    };

    let summary = storage_summary(db, &tracks);
    let hires_tracks = songs.iter().filter(|s| s.is_hires).count() as u32;
    eprintln!("cinder-ffi: hi-res tracks: {hires_tracks}");
    let (folders, folder_roots) = if is_shared.iter().any(|&x| x) {
        let keep = |i: &usize| !is_shared[*i];
        let real_tracks: Vec<cinder_db::Track> = (0..tracks.len())
            .filter(keep)
            .map(|i| tracks[i].clone())
            .collect();
        let real_songs: Vec<SongRow> = (0..songs.len())
            .filter(keep)
            .map(|i| songs[i].clone())
            .collect();
        build_folders(&real_tracks, &real_songs)
    } else {
        build_folders(&tracks, &songs)
    };

    // The breakdown of the boot dead time. `sql` is the four DB calls; `model` is everything this
    // function does with their results. If `sql` dominates on device the way it does on the host,
    // the lever is SQLite configuration (see `Db::open`, which sets no PRAGMAs); if `model`
    // dominates, it is this function's own BTreeMap work and no PRAGMA will touch it.
    let ms_sql = ms_years + ms_tracks + ms_album_order + ms_albums;
    let ms_all = t_total.elapsed().as_millis();
    eprintln!(
        "cinder-ffi: build_library {ms_all} ms = sql {ms_sql} (years {ms_years}, tracks \
         {ms_tracks}, album_order {ms_album_order}, albums {ms_albums}) + model {} (grouping \
         {ms_album_group})",
        ms_all.saturating_sub(ms_sql)
    );

    // `thumbs` is filled separately by start_art_cache: the disk cache load is I/O, not model
    // building, and the rest arrives asynchronously from the decoder thread.
    // Automatic covers for the Sony rows built above. Ours get theirs in `user_playlist_rows`,
    // and `refresh_playlists` re-runs this over the merged list on every edit.
    let mut playlists: Vec<cinder_ui::model::PlaylistRow> = playlists;
    auto_playlist_covers(&mut playlists);

    let mut lib = cinder_ui::Library {
        summary,
        stats: track_stats,
        songs,
        album_groups,
        artists,
        playlists,
        thumbs: Default::default(),
        // Filled by `refresh_playlists`, which runs right after this — and normally empty, since
        // only a playlist whose cover is a PICTURE FILE needs one.
        playlist_thumbs: Default::default(),
        cover_pictures: Vec::new(),
        playlist_pictures: Default::default(),
        artist_photos: Default::default(),
        genres,
        hires_tracks,
        filter_genre: None,
        filter_hires: false,
        // The App's preference, applied by `set_library` (which also re-sorts the artists).
        ignore_the: false,
        ranks: Default::default(),
        folders,
        folder_roots,
        channels: Vec::new(),
        sensme_tracks: 0,
        // The App's layouts, applied by `set_library` like `ignore_the`.
        views: Default::default(),
        folders_tab: false,
        covers: Default::default(),
        books,
    };
    // The channel table is a reverse-engineering finding and lives with the akeys that produced it
    // (`cinder_db::SENSME_CHANNELS`); cinder-ui is DB-free and is handed it. `build_channels` is
    // the only implementation, shared with the host preview's sample library.
    lib.build_channels(&cinder_db::SENSME_CHANNELS);
    eprintln!(
        "cinder-ffi: sensme: {} analysed tracks in {} channels ({ms_sensme} ms)",
        lib.sensme_tracks,
        lib.channels.len()
    );
    lib
}

/// Build the FOLDER browse tree from the tracks' absolute paths.
///
/// `cinder_db` already resolves every track to an absolute path (its `dirs` map walks `parent_id`
/// up to a storage root), so the tree is derived from the paths rather than queried again — one
/// pass, no extra SQL, and it cannot disagree with what Track information shows.
///
/// Directories with NO tracks anywhere below them do not appear. They are not browsable places on
/// a music player, and on this device the file tree is full of them (cover-art folders, the MTP
/// scratch directories) — showing them would bury the four folders that matter.
pub fn build_folders(
    tracks: &[cinder_db::Track],
    songs: &[cinder_ui::model::SongRow],
) -> (Vec<cinder_ui::model::FolderRow>, Vec<usize>) {
    use cinder_ui::model::FolderRow;
    use std::collections::HashMap;

    let mut idx: HashMap<String, usize> = HashMap::new();
    let mut out: Vec<FolderRow> = Vec::new();

    // Intern one path, creating every missing ancestor above it. Returns its index.
    fn intern(path: &str, idx: &mut HashMap<String, usize>, out: &mut Vec<FolderRow>) -> usize {
        if let Some(i) = idx.get(path) {
            return *i;
        }
        let cut = path.rfind('/').unwrap_or(0);
        // A path with no slash above the first character is a root: `/contents` has its slash at
        // 0, and slicing to 0 would make the parent the empty string, i.e. a phantom root above
        // every mount.
        let parent = (cut > 0).then(|| intern(&path[..cut], idx, out));
        let name = if parent.is_some() {
            &path[cut + 1..]
        } else {
            path
        };
        let me = out.len();
        out.push(FolderRow {
            path: path.to_string(),
            name: name.to_string(),
            parent,
            subdirs: Vec::new(),
            tracks: Vec::new(),
            total: 0,
        });
        idx.insert(path.to_string(), me);
        if let Some(p) = parent {
            out[p].subdirs.push(me);
        }
        me
    }

    // `songs` is built from `tracks` in the same order (see the loop in build_library), so the two
    // index together — which is what lets the tree hold ready-made SongRows instead of rebuilding
    // them, and keeps a folder row identical to the same track's row anywhere else.
    // Each track with its file name, which is what a folder sorts by (below).
    let mut files: Vec<Vec<(&str, &cinder_ui::model::SongRow)>> = Vec::new();
    for (t, row) in tracks.iter().zip(songs.iter()) {
        let Some(cut) = t.filename.rfind('/') else {
            continue;
        };
        if cut == 0 {
            continue; // a bare "/name" — no directory to file it under
        }
        let dir = intern(&t.filename[..cut], &mut idx, &mut out);
        files.resize_with(out.len(), Vec::new);
        files[dir].push((&t.filename[cut + 1..], row));
    }
    // Tracks in FILE NAME order, as Sony's folder view has them (#45) — on a music folder that is
    // track order, and it is the order the files are actually in on the volume, which is the whole
    // point of browsing this way. This said so for months while sorting by track number and title.
    for (dir, mut f) in files.into_iter().enumerate() {
        f.sort_by(|a, b| cinder_ui::collate::cmp(a.0, b.0));
        out[dir].tracks = f.into_iter().map(|(_, row)| row.clone()).collect();
    }

    // Totals: every directory counts its own tracks plus everything below. Walked from each
    // directory UP to its root rather than recursively down, so a pathological depth cannot blow
    // the stack — and the depth is bounded anyway by the `intern` recursion above it.
    for i in 0..out.len() {
        let n = out[i].tracks.len() as u32;
        if n == 0 {
            continue;
        }
        let mut cur = Some(i);
        let mut guard = 0;
        while let Some(c) = cur {
            out[c].total += n;
            cur = out[c].parent;
            guard += 1;
            if guard > 64 {
                break;
            }
        }
    }

    // Subdirectories alphabetically, in the Library's one collation (case and accents folded).
    let names: Vec<String> = out.iter().map(|f| f.name.clone()).collect();
    for i in 0..out.len() {
        let mut subs = std::mem::take(&mut out[i].subdirs);
        subs.sort_by(|a, b| cinder_ui::collate::cmp(&names[*a], &names[*b]));
        // Prune the branches that hold no music at all.
        subs.retain(|s| out[*s].total > 0);
        out[i].subdirs = subs;
    }

    let mut roots: Vec<usize> = (0..out.len())
        .filter(|i| out[*i].parent.is_none() && out[*i].total > 0)
        .collect();
    roots.sort_by(|a, b| cinder_ui::collate::cmp(&names[*a], &names[*b]));
    eprintln!(
        "cinder-ffi: folders: {} dirs, {} root(s)",
        out.len(),
        roots.len()
    );
    (out, roots)
}

/// Is a release-year label worth printing? Sony's `releaseyears` table spells "unknown" as `"0"`.
pub fn is_real_year(y: &str) -> bool {
    let y = y.trim();
    !y.is_empty() && y.parse::<i64>().map_or(true, |n| n > 0)
}

/// One DB track as the owned UI row. Shared by the library build and by every play action, so a
/// track shows the same title/artist/duration wherever it appears. `year` is left 0: it is only a
/// Songs-tab sort key and resolving it needs the release-year FK map, which the library build has
/// and a play action does not.
pub fn song_row_of(t: &cinder_db::Track) -> cinder_ui::model::SongRow {
    let title = if t.title.is_empty() {
        t.filename.rsplit('/').next().unwrap_or("").to_string()
    } else {
        t.title.clone()
    };
    let art = if t.album.is_empty() {
        title.clone()
    } else {
        t.album.clone()
    };
    cinder_ui::model::SongRow {
        title,
        artist: t.artist.clone(),
        dur: t.duration_raw.map(fmt_time).unwrap_or_default(),
        art,
        object_id: t.object_id,
        album_id: t.album_id.unwrap_or(0),
        disc: t.disc_no as i32,
        track: t.track_no as i32,
        added: t.added,
        year: 0,
        genre_id: t.genre_id.unwrap_or(0),
        is_hires: t.is_hires,
        // Only when it DIFFERS from the track artist: the common row then carries an empty string,
        // which allocates nothing (`SongRow::group_artist` supplies the fallback).
        album_artist: {
            let aa = t.album_artist.trim();
            if aa.is_empty() || aa == t.artist.trim() {
                String::new()
            } else {
                t.album_artist.clone()
            }
        },
        format: cinder_ui::model::Format::of_path(&t.filename),
        // Filled by `build_library` from the SensMe map, which is keyed by object id — this
        // builder only sees the track row. Every copy that is not in `Library::songs` (an album's
        // track list, a playlist's) leaves it 0, and nothing but `build_channels` reads it.
        sensme: 0,
    }
}

/// Give every playlist without a chosen cover the art of its first member track.
///
/// This is what makes a playlist look like something with nobody having done anything, and it is
/// the only cover Sony's own playlists can ever have — they live in a database this app must not
/// write, so there is nowhere to record a choice.
///
/// It runs over the MERGED rows rather than inside each builder, because "the first track that
/// actually resolved" is a property of the finished row: a playlist whose first two files are
/// missing should show the third one's cover, not a gradient.
pub fn auto_playlist_covers(rows: &mut [cinder_ui::model::PlaylistRow]) {
    for row in rows.iter_mut() {
        if row.cover_album_id == 0 {
            row.cover_album_id = row.track_list.first().map_or(0, |t| t.album_id);
        }
    }
}

/// The CUE tracks of an album's files, in order, or nothing when none of its files has a sheet.
/// A file without one keeps a row of its own between the CUE tracks, so nothing goes missing.
pub fn cue_rows(
    files: &[cinder_ui::model::SongRow],
    long_files: &std::collections::BTreeMap<i64, (String, i64)>,
) -> Vec<cinder_ui::model::CueRow> {
    let mut out = Vec::new();
    let mut any = false;
    for (fi, s) in files.iter().enumerate() {
        let sheet = long_files
            .get(&s.object_id)
            .and_then(|(path, _)| cue::load_for(path));
        match sheet {
            Some(sh) => {
                any = true;
                let len = long_files.get(&s.object_id).map_or(0, |(_, d)| *d);
                for (i, e) in sh.entries.iter().enumerate() {
                    let end = sh.entries.get(i + 1).map_or(len, |n| n.start_ms);
                    out.push(cinder_ui::model::CueRow {
                        file: fi,
                        title: e.title.clone(),
                        start_ms: e.start_ms,
                        dur: if end > e.start_ms {
                            fmt_time(end - e.start_ms)
                        } else {
                            String::new()
                        },
                    });
                }
            }
            None => out.push(cinder_ui::model::CueRow {
                file: fi,
                title: s.title.clone(),
                start_ms: 0,
                dur: s.dur.clone(),
            }),
        }
    }
    if any {
        out
    } else {
        Vec::new()
    }
}

/// Format milliseconds as `M:SS` (or `H:MM:SS`). `duration_raw` units are assumed ms —
/// calibrate on device (see cinder-db notes); only this one place needs changing if not ms.
pub fn fmt_time(ms: i64) -> String {
    let total = ms.max(0) / 1000;
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}
