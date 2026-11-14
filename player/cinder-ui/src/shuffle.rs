//! Shuffle by album and by artist — the grouping half of `docs/SPEC_queue_v2.md` §4.
//!
//! Shuffle by SONGS deals every track at random, which is what Cinder has always done. By ALBUMS
//! it deals whole albums at random and plays each one in its own order; by ARTISTS, whole artists,
//! each one album after another. It is a setting (Settings ▸ Shuffle), not a fourth state on the
//! transport button: the button stays the on/off switch it has always been, and the setting says
//! what "on" means. The default is Songs, so nothing changes until the owner picks otherwise.
//! SPREAD is Songs with each artist's songs spaced out through the list, so one artist does not
//! come up three times running (what a plain random deal does, and what people hear as "not random").
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
    Spread,
}

impl ShuffleBy {
    pub const ALL: [ShuffleBy; 4] = [
        ShuffleBy::Songs,
        ShuffleBy::Albums,
        ShuffleBy::Artists,
        ShuffleBy::Spread,
    ];

    /// The settings-file word (`shuffle_by=`). A word, so a fourth mode cannot shift what an
    /// existing file means.
    pub fn token(self) -> &'static str {
        match self {
            ShuffleBy::Songs => "songs",
            ShuffleBy::Albums => "albums",
            ShuffleBy::Artists => "artists",
            ShuffleBy::Spread => "spread",
        }
    }

    pub fn from_token(s: &str) -> ShuffleBy {
        crate::from_token(&Self::ALL, Self::token, s)
    }

    /// The Settings row's value.
    pub fn label(self) -> &'static str {
        match self {
            ShuffleBy::Songs => crate::i18n::text("SONGS"),
            ShuffleBy::Albums => crate::i18n::text("ALBUMS"),
            ShuffleBy::Artists => crate::i18n::text("ARTISTS"),
            ShuffleBy::Spread => crate::i18n::text("SPREAD"),
        }
    }

    /// What the toast says after a deal.
    pub fn toast(self) -> &'static str {
        match self {
            ShuffleBy::Songs => crate::i18n::text("Shuffled"),
            ShuffleBy::Albums => crate::i18n::text("Shuffled by album"),
            ShuffleBy::Artists => crate::i18n::text("Shuffled by artist"),
            ShuffleBy::Spread => crate::i18n::text("Shuffled, artists spread out"),
        }
    }

    pub fn next(self) -> ShuffleBy {
        crate::next_in(&Self::ALL, self)
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
    if album_id > 0 {
        album_id as u64
    } else {
        (object_id as u64) | (1 << 63)
    }
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

/// The one random generator for every shuffle and random pick (xorshift64*). Not cryptographic:
/// a play order has nothing to protect, and a dependency for it would be the only one. The seed is
/// the caller's, so cinder-ui stays clock-free and its tests replay exactly; the shell seeds it
/// from the clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rng(u64);

impl Rng {
    /// Zero is moved off: xorshift never leaves it.
    pub fn new(seed: u64) -> Rng {
        Rng(if seed == 0 {
            0x9E37_79B9_7F4A_7C15
        } else {
            seed
        })
    }

    #[allow(clippy::should_implement_trait)] // an endless generator, not an iterator
    pub fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// A number in `0..n` (`n` > 0).
    pub fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    /// A number in `[0, 1)`.
    pub fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Fisher-Yates.
    pub fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            v.swap(i, self.below(i + 1));
        }
    }
}

/// `0..w.len()` in a random order where a heavier item tends to come sooner: item `i` comes
/// first with chance `w[i] / Σw` (exponential keys, Efraimidis–Spirakis).
pub fn weighted_order(w: &[f64], rng: &mut Rng) -> Vec<usize> {
    let mut keyed: Vec<(f64, usize)> = w
        .iter()
        .enumerate()
        .map(|(i, &wi)| (-(1.0 - rng.unit()).ln() / wi.max(1e-9), i))
        .collect();
    keyed.sort_by(|a, b| a.0.total_cmp(&b.0));
    keyed.into_iter().map(|(_, i)| i).collect()
}

/// How strongly an album of `tracks` songs is drawn early in a by-album deal: its track count, up
/// to 8. A full album comes up about eight times as often as a single, so singles and EPs (a fifth
/// of the owner's library) drift towards the end instead of turning up every few albums.
pub fn album_weight(tracks: usize) -> f64 {
    tracks.clamp(1, 8) as f64
}

/// `v` in the order `deal` gave.
pub fn apply<T>(v: Vec<T>, order: &[usize]) -> Vec<T> {
    let mut old: Vec<Option<T>> = v.into_iter().map(Some).collect();
    order.iter().filter_map(|&i| old[i].take()).collect()
}

/// Deal `rows` into a new order: a permutation of `0..rows.len()`.
///
/// * **Songs** — Fisher-Yates over every row.
/// * **Spread** — every row at random, each artist's rows spaced evenly through the list: an
///   artist with n rows gets one in each n-th of it, at a random offset, in random order.
/// * **Albums / Artists** — rows are grouped by album or by artist; the GROUPS are dealt at random
///   (albums weighted by `album_weight`, so full albums tend to come before singles)
///   and each keeps its own order: an album in disc and track order, an artist album by album
///   (in the order the albums first appear) and each album in track order.
///
/// `lead` is the playing row, when there is one. It comes first whatever the mode, and in a
/// grouped deal the rest of ITS group follows it — the album carries on from the next track, then
/// comes round to the tracks before it — so turning shuffle on mid-album does not cut the album
/// off. A deal with fewer than two groups has nothing to permute, and falls back to Songs rather
/// than handing back the order it was given. Returns the order and the mode it really dealt by
/// (Songs after that fallback), which is what the toast should say.
pub fn deal(
    rows: &[Row],
    by: ShuffleBy,
    lead: Option<usize>,
    rng: &mut Rng,
) -> (Vec<usize>, ShuffleBy) {
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
        rng.shuffle(&mut order);
        return (lead.into_iter().chain(order).collect(), ShuffleBy::Songs);
    }
    if by == ShuffleBy::Spread {
        let mut placed: Vec<(f64, usize)> = Vec::with_capacity(n);
        for (_, members) in &mut groups {
            members.retain(|&i| Some(i) != lead);
            rng.shuffle(members);
            let (len, offset) = (members.len() as f64, rng.unit());
            placed.extend(
                members
                    .iter()
                    .enumerate()
                    .map(|(k, &i)| ((k as f64 + offset) / len, i)),
            );
        }
        placed.sort_by(|a, b| a.0.total_cmp(&b.0));
        return (
            lead.into_iter()
                .chain(placed.into_iter().map(|(_, i)| i))
                .collect(),
            by,
        );
    }
    // Inside a group: albums in order of first appearance, then disc and track. Stable, so rows
    // with no track numbers keep the order they came in.
    for (_, members) in &mut groups {
        let mut album_rank: std::collections::HashMap<u64, usize> =
            std::collections::HashMap::new();
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
    if by == ShuffleBy::Albums {
        let w: Vec<f64> = rest
            .iter()
            .map(|&g| album_weight(groups[g].1.len()))
            .collect();
        rest = weighted_order(&w, rng)
            .into_iter()
            .map(|i| rest[i])
            .collect();
    } else {
        rng.shuffle(&mut rest);
    }
    for g in rest {
        out.extend(groups[g].1.iter().copied());
    }
    (out, by)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Three albums of four tracks, by two artists, in album order.
    fn rows() -> Vec<Row> {
        let mut v = Vec::new();
        for (album, artist) in [(1u64, 10u64), (2, 20), (3, 10)] {
            for t in 1..=4 {
                v.push(Row {
                    album,
                    artist,
                    disc: 1,
                    track: t,
                });
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
                let (o, _) = deal(&r, by, lead, &mut Rng::new(7));
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
            let (o, _) = deal(&r, ShuffleBy::Albums, None, &mut Rng::new(seed));
            for chunk in o.chunks(4) {
                let album = r[chunk[0]].album;
                assert!(
                    chunk.iter().all(|&i| r[i].album == album),
                    "an album was split: {o:?}"
                );
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
        let (o, _) = deal(&r, ShuffleBy::Artists, None, &mut Rng::new(3));
        let artists: Vec<u64> = o.iter().map(|&i| r[i].artist).collect();
        let changes = artists.windows(2).filter(|w| w[0] != w[1]).count();
        assert_eq!(
            changes, 1,
            "two artists, so exactly one change of artist: {artists:?}"
        );
        let tens: Vec<(u64, i32)> = o
            .iter()
            .filter(|&&i| r[i].artist == 10)
            .map(|&i| (r[i].album, r[i].track))
            .collect();
        assert_eq!(
            tens,
            [
                (1, 1),
                (1, 2),
                (1, 3),
                (1, 4),
                (3, 1),
                (3, 2),
                (3, 3),
                (3, 4)
            ]
        );
        assert_eq!(
            artist_key("The Cure"),
            artist_key(" the cure "),
            "case-blind"
        );
    }

    /// Mid-album: the playing track leads, the rest of its album follows from the next track and
    /// comes round to the ones before it, then the other albums.
    #[test]
    fn the_playing_album_carries_on_after_the_playing_track() {
        let r = rows();
        let (o, _) = deal(&r, ShuffleBy::Albums, Some(5), &mut Rng::new(11)); // album 2, track 2
        assert_eq!(&o[..4], &[5, 6, 7, 4]);
        assert!(o[4..].iter().all(|&i| r[i].album != 2));
    }

    /// One album only: nothing to deal between, so it shuffles the songs instead of returning the
    /// order it was handed (which would look like a shuffle button that does nothing).
    #[test]
    fn a_single_group_falls_back_to_songs() {
        let r: Vec<Row> = (1..=10)
            .map(|t| Row {
                album: 1,
                artist: 1,
                disc: 1,
                track: t,
            })
            .collect();
        let (o, by) = deal(&r, ShuffleBy::Albums, None, &mut Rng::new(5));
        assert_eq!(by, ShuffleBy::Songs, "and says so");
        assert!(is_permutation(&o, 10));
        assert_ne!(o, (0..10).collect::<Vec<_>>());
    }

    /// Spread: two artists with six songs each never play three of one artist running, and a
    /// plain deal of the same rows does, for some seed.
    #[test]
    fn spread_spaces_each_artist_out() {
        let r: Vec<Row> = (0..12)
            .map(|i| Row {
                album: i,
                artist: i % 2 + 1,
                disc: 1,
                track: 1,
            })
            .collect();
        let run = |o: &[usize]| {
            o.windows(3)
                .any(|w| w.iter().all(|&i| r[i].artist == r[w[0]].artist))
        };
        let mut plain_clumps = false;
        for seed in 1..50 {
            let (o, by) = deal(&r, ShuffleBy::Spread, Some(3), &mut Rng::new(seed));
            assert_eq!(by, ShuffleBy::Spread);
            assert!(is_permutation(&o, 12) && o[0] == 3);
            assert!(!run(&o[1..]), "seed {seed}: {o:?}");
            plain_clumps |= run(&deal(&r, ShuffleBy::Songs, None, &mut Rng::new(seed)).0);
        }
        assert!(plain_clumps, "the test rows can clump at all");
    }

    /// By albums, a ten-song album leads far more often than a single: over 400 deals of one
    /// album and nine singles the album is first well over a third of the time (uniform: a tenth).
    #[test]
    fn full_albums_tend_to_come_before_singles() {
        let mut r: Vec<Row> = (1..=10)
            .map(|t| Row {
                album: 1,
                artist: 1,
                disc: 1,
                track: t,
            })
            .collect();
        r.extend((2..=10).map(|a| Row {
            album: a,
            artist: a,
            disc: 1,
            track: 1,
        }));
        let mut rng = Rng::new(42);
        let first = (0..400)
            .filter(|_| r[deal(&r, ShuffleBy::Albums, None, &mut rng).0[0]].album == 1)
            .count();
        assert!(first > 140, "the album led {first} of 400");
        let w = weighted_order(&[1.0, 1.0, 1.0], &mut rng);
        assert!(is_permutation(&w, 3));
    }

    #[test]
    fn tokens_round_trip() {
        for by in ShuffleBy::ALL {
            assert_eq!(ShuffleBy::from_token(by.token()), by);
        }
        assert_eq!(ShuffleBy::from_token("nonsense"), ShuffleBy::Songs);
        assert_eq!(album_key(0, 5), album_key(0, 5));
        assert_ne!(
            album_key(0, 5),
            album_key(0, 6),
            "albumless tracks are singletons"
        );
    }
}
