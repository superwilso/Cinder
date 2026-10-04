//! Shuffle by album and by artist — the grouping half of `docs/SPEC_queue_v2.md` §4.
//!
//! Shuffle by SONGS deals every track at random, which is what Cinder has always done. By ALBUMS
//! it deals whole albums at random and plays each one in its own order; by ARTISTS, whole artists,
//! each one album after another. It is a setting (Settings ▸ Shuffle), not a fourth state on the
//! transport button: the button stays the on/off switch it has always been, and the setting says
//! what "on" means. The default is Songs, so nothing changes until the owner picks otherwise.
//!
//! Pure and allocation-light: the callers — `App::queue_shuffle` on `SongRow`s and the shell's
//! library bands on DB tracks — describe their rows as [`Row`]s and get back a permutation, so one
//! implementation serves both and the two can never deal differently.

/// What a shuffle deals.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShuffleBy {
    #[default]
    Songs,
    Albums,
    Artists,
}

impl ShuffleBy {
    pub const ALL: [ShuffleBy; 3] = [ShuffleBy::Songs, ShuffleBy::Albums, ShuffleBy::Artists];

    /// The settings-file word (`shuffle_by=`). A word, so a fourth mode cannot shift what an
    /// existing file means.
    pub fn token(self) -> &'static str {
        match self {
            ShuffleBy::Songs => "songs",
            ShuffleBy::Albums => "albums",
            ShuffleBy::Artists => "artists",
        }
    }

    pub fn from_token(s: &str) -> ShuffleBy {
        match s.trim() {
            "albums" => ShuffleBy::Albums,
            "artists" => ShuffleBy::Artists,
            _ => ShuffleBy::Songs,
        }
    }

    /// The Settings row's value.
    pub fn label(self) -> &'static str {
        match self {
            ShuffleBy::Songs => "SONGS",
            ShuffleBy::Albums => "ALBUMS",
            ShuffleBy::Artists => "ARTISTS",
        }
    }

    /// What the toast says after a deal.
    pub fn toast(self) -> &'static str {
        match self {
            ShuffleBy::Songs => "Shuffled",
            ShuffleBy::Albums => "Shuffled by album",
            ShuffleBy::Artists => "Shuffled by artist",
        }
    }

    pub fn next(self) -> ShuffleBy {
        match self {
            ShuffleBy::Songs => ShuffleBy::Albums,
            ShuffleBy::Albums => ShuffleBy::Artists,
            ShuffleBy::Artists => ShuffleBy::Songs,
        }
    }
}

/// One row to deal, described by the keys a grouped shuffle needs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Row {
    /// The album: its id, or — for a track with no album row behind it — something unique to the
    /// track, so albumless tracks are singletons rather than one giant "no album" album.
    pub album: u64,
    /// The artist the track is FILED under (`SongRow::group_artist`), folded and hashed.
    pub artist: u64,
    pub disc: i32,
    pub track: i32,
}

/// The album key for a track: its album id, or its own id when it has none.
pub fn album_key(album_id: i64, object_id: i64) -> u64 {
    if album_id > 0 { album_id as u64 } else { (object_id as u64) | (1 << 63) }
}

/// The artist key: case-blind FNV-1a over the name, so "The Cure" and "the cure" are one artist,
/// the way the Library's collation treats them.
pub fn artist_key(name: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for c in name.trim().chars().flat_map(char::to_lowercase) {
        let mut b = [0u8; 4];
        for byte in c.encode_utf8(&mut b).bytes() {
            h ^= byte as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    h
}

/// Deal `rows` into a new order: a permutation of `0..rows.len()`.
///
/// * **Songs** — Fisher-Yates over every row.
/// * **Albums / Artists** — rows are grouped by album or by artist; the GROUPS are dealt at random
///   and each keeps its own order: an album in disc and track order, an artist album by album
///   (in the order the albums first appear) and each album in track order.
///
/// `lead` is the playing row, when there is one. It comes first whatever the mode, and in a
/// grouped deal the rest of ITS group follows it — the album carries on from the next track, then
/// comes round to the tracks before it — so turning shuffle on mid-album does not cut the album
/// off. A deal with fewer than two groups has nothing to permute, and falls back to Songs rather
/// than handing back the order it was given.
///
/// `rnd(n)` returns a number in `0..n`; the caller owns the generator, so tests are reproducible.
pub fn deal(rows: &[Row], by: ShuffleBy, lead: Option<usize>, rnd: &mut dyn FnMut(usize) -> usize) -> Vec<usize> {
    let n = rows.len();
    let lead = lead.filter(|&l| l < n);
    let key = |r: &Row| match by {
        ShuffleBy::Albums => r.album,
        _ => r.artist,
    };
    // Groups in order of first appearance, with their member indices.
    let mut groups: Vec<(u64, Vec<usize>)> = Vec::new();
    if by != ShuffleBy::Songs {
        let mut at: std::collections::HashMap<u64, usize> = std::collections::HashMap::new();
        for (i, r) in rows.iter().enumerate() {
            let k = key(r);
            let g = *at.entry(k).or_insert_with(|| {
                groups.push((k, Vec::new()));
                groups.len() - 1
            });
            groups[g].1.push(i);
        }
    }
    if groups.len() < 2 {
        // Songs, or a grouped deal with nothing to group: every row but the lead, at random.
        let mut order: Vec<usize> = (0..n).filter(|&i| Some(i) != lead).collect();
        for i in (1..order.len()).rev() {
            order.swap(i, rnd(i + 1));
        }
        return lead.into_iter().chain(order).collect();
    }
    // Inside a group: albums in order of first appearance, then disc and track. Stable, so rows
    // with no track numbers keep the order they came in.
    for (_, members) in &mut groups {
        let mut album_rank: std::collections::HashMap<u64, usize> = std::collections::HashMap::new();
        for &i in members.iter() {
            let len = album_rank.len();
            album_rank.entry(rows[i].album).or_insert(len);
        }
        members.sort_by_key(|&i| (album_rank[&rows[i].album], rows[i].disc, rows[i].track));
    }
    let lead_group = lead.and_then(|l| groups.iter().position(|(_, m)| m.contains(&l)));
    let mut out: Vec<usize> = Vec::with_capacity(n);
    let mut rest: Vec<usize> = (0..groups.len()).collect();
    if let (Some(l), Some(g)) = (lead, lead_group) {
        let m = &groups[g].1;
        let p = m.iter().position(|&i| i == l).unwrap_or(0);
        out.extend(m[p..].iter().chain(m[..p].iter()).copied());
        rest.retain(|&x| x != g);
    }
    for i in (1..rest.len()).rev() {
        rest.swap(i, rnd(i + 1));
    }
    for g in rest {
        out.extend(groups[g].1.iter().copied());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lcg(seed: u64) -> impl FnMut(usize) -> usize {
        let mut x = seed;
        move |n| {
            x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((x >> 33) as usize) % n
        }
    }

    /// Three albums of four tracks, by two artists, in album order.
    fn rows() -> Vec<Row> {
        let mut v = Vec::new();
        for (album, artist) in [(1u64, 10u64), (2, 20), (3, 10)] {
            for t in 1..=4 {
                v.push(Row { album, artist, disc: 1, track: t });
            }
        }
        v
    }

    fn is_permutation(order: &[usize], n: usize) -> bool {
        let mut s = order.to_vec();
        s.sort_unstable();
        s == (0..n).collect::<Vec<_>>()
    }

    /// Every mode is a permutation — nothing dropped, nothing doubled — with and without a lead.
    #[test]
    fn every_deal_is_a_permutation() {
        let r = rows();
        for by in ShuffleBy::ALL {
            for lead in [None, Some(0), Some(6), Some(11)] {
                let o = deal(&r, by, lead, &mut lcg(7));
                assert!(is_permutation(&o, r.len()), "{by:?} lead {lead:?}: {o:?}");
                if let Some(l) = lead {
                    assert_eq!(o[0], l, "{by:?}: the playing row stays first");
                }
            }
        }
    }

    /// By albums: each album arrives whole and in track order, and the albums are not simply left
    /// in the order they came in for every seed.
    #[test]
    fn albums_arrive_whole_and_in_order() {
        let r = rows();
        let mut orders = std::collections::HashSet::new();
        for seed in 0..20 {
            let o = deal(&r, ShuffleBy::Albums, None, &mut lcg(seed));
            for chunk in o.chunks(4) {
                let album = r[chunk[0]].album;
                assert!(chunk.iter().all(|&i| r[i].album == album), "an album was split: {o:?}");
                let tracks: Vec<i32> = chunk.iter().map(|&i| r[i].track).collect();
                assert_eq!(tracks, [1, 2, 3, 4]);
            }
            orders.insert(o.chunks(4).map(|c| r[c[0]].album).collect::<Vec<_>>());
        }
        assert!(orders.len() > 1, "the album order never changed");
    }

    /// By artists: an artist's two albums play back to back, each in order, and a compilation
    /// (tracks by several artists, one album artist) is ONE group because the key is the album
    /// artist the caller hashed.
    #[test]
    fn artists_keep_their_albums_together() {
        let r = rows();
        let o = deal(&r, ShuffleBy::Artists, None, &mut lcg(3));
        let artists: Vec<u64> = o.iter().map(|&i| r[i].artist).collect();
        let changes = artists.windows(2).filter(|w| w[0] != w[1]).count();
        assert_eq!(changes, 1, "two artists, so exactly one change of artist: {artists:?}");
        let tens: Vec<(u64, i32)> = o.iter().filter(|&&i| r[i].artist == 10).map(|&i| (r[i].album, r[i].track)).collect();
        assert_eq!(tens, [(1, 1), (1, 2), (1, 3), (1, 4), (3, 1), (3, 2), (3, 3), (3, 4)]);
        assert_eq!(artist_key("The Cure"), artist_key(" the cure "), "case-blind");
    }

    /// Mid-album: the playing track leads, the rest of its album follows from the next track and
    /// comes round to the ones before it, then the other albums.
    #[test]
    fn the_playing_album_carries_on_after_the_playing_track() {
        let r = rows();
        let o = deal(&r, ShuffleBy::Albums, Some(5), &mut lcg(11)); // album 2, track 2
        assert_eq!(&o[..4], &[5, 6, 7, 4]);
        assert!(o[4..].iter().all(|&i| r[i].album != 2));
    }

    /// One album only: nothing to deal between, so it shuffles the songs instead of returning the
    /// order it was handed (which would look like a shuffle button that does nothing).
    #[test]
    fn a_single_group_falls_back_to_songs() {
        let r: Vec<Row> = (1..=10).map(|t| Row { album: 1, artist: 1, disc: 1, track: t }).collect();
        let o = deal(&r, ShuffleBy::Albums, None, &mut lcg(5));
        assert!(is_permutation(&o, 10));
        assert_ne!(o, (0..10).collect::<Vec<_>>());
    }

    #[test]
    fn tokens_round_trip() {
        for by in ShuffleBy::ALL {
            assert_eq!(ShuffleBy::from_token(by.token()), by);
        }
        assert_eq!(ShuffleBy::from_token("nonsense"), ShuffleBy::Songs);
        assert_eq!(album_key(0, 5), album_key(0, 5));
        assert_ne!(album_key(0, 5), album_key(0, 6), "albumless tracks are singletons");
    }
}
