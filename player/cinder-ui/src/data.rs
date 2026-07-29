//! Sample library/device data shared by screens (host PNG + sim). Mirrors the
//! prototype constants in finalists-a-library.jsx / finalists-shared.jsx.

pub struct Song {
    pub t: &'static str,
    pub a: &'static str,
    pub d: &'static str,
    pub art: &'static str,
}

pub const SONGS: &[Song] = &[
    Song { t: "Atlas Hands", a: "Benjamin Francis Leftwich", d: "4:32", art: "kind" },
    Song { t: "Box of Stones", a: "Benjamin Francis Leftwich", d: "3:58", art: "kind" },
    Song { t: "Harvest Moon", a: "Cold Stone & Sea", d: "5:03", art: "harvest" },
    Song { t: "Midnight Arcade", a: "Neon Cartography", d: "4:11", art: "midnight" },
    Song { t: "Ferns", a: "Hollow Pines", d: "3:24", art: "ferns" },
    Song { t: "Halcyon Days", a: "Vesper Lane", d: "4:47", art: "halcyon" },
    Song { t: "Bloom", a: "Petal & Wire", d: "3:36", art: "bloom" },
    Song { t: "Prism Break", a: "Glass Atlas", d: "4:02", art: "prism" },
];

pub struct Album {
    pub n: &'static str,
    pub k: u32,
    pub y: &'static str,
    pub art: &'static str,
}

pub struct AlbumGroup {
    pub artist: &'static str,
    pub albums: &'static [Album],
}

pub const ALBUM_GROUPS: &[AlbumGroup] = &[
    AlbumGroup {
        artist: "Benjamin Francis Leftwich",
        albums: &[
            Album { n: "Last Smoke Before the Snowstorm", k: 12, y: "2011", art: "kind" },
            Album { n: "After the Rain", k: 10, y: "2016", art: "atlas" },
        ],
    },
    AlbumGroup {
        artist: "Cold Stone & Sea",
        albums: &[
            Album { n: "Harvest Moon", k: 10, y: "2019", art: "harvest" },
            Album { n: "Static Lines", k: 9, y: "2022", art: "static" },
        ],
    },
    AlbumGroup {
        artist: "Glass Atlas",
        albums: &[Album { n: "Prism Break", k: 10, y: "2021", art: "prism" }],
    },
    AlbumGroup {
        artist: "Neon Cartography",
        albums: &[Album { n: "Midnight Arcade", k: 11, y: "2020", art: "midnight" }],
    },
];

pub struct Artist {
    pub n: &'static str,
    pub al: u32,
    pub tr: u32,
    pub arts: &'static [&'static str],
}

pub const ARTISTS: &[Artist] = &[
    Artist { n: "Benjamin Francis Leftwich", al: 3, tr: 34, arts: &["kind", "atlas"] },
    Artist { n: "Cold Stone & Sea", al: 2, tr: 21, arts: &["harvest", "static"] },
    Artist { n: "Glass Atlas", al: 1, tr: 10, arts: &["prism"] },
    Artist { n: "Hollow Pines", al: 2, tr: 19, arts: &["ferns", "cassette"] },
    Artist { n: "Neon Cartography", al: 4, tr: 46, arts: &["midnight", "prism"] },
    Artist { n: "Petal & Wire", al: 1, tr: 8, arts: &["bloom"] },
    Artist { n: "Vesper Lane", al: 2, tr: 26, arts: &["halcyon", "bloom"] },
];

pub struct Playlist {
    pub n: &'static str,
    pub k: u32,
    pub art: &'static str,
}

pub const PLAYLISTS: &[Playlist] = &[
    Playlist { n: "Liked Songs", k: 214, art: "bloom" },
    Playlist { n: "Night Drives", k: 32, art: "midnight" },
    Playlist { n: "Acoustic Mornings", k: 48, art: "ferns" },
    Playlist { n: "Hi-Res Showcase", k: 26, art: "prism" },
];

pub struct Paired {
    pub name: &'static str,
    pub kind: &'static str,
}

pub const PAIRED: &[Paired] = &[
    Paired { name: "WF-1000XM4", kind: "Earbuds · LDAC" },
    Paired { name: "SRS-XB23", kind: "Speaker · AAC" },
    Paired { name: "Car · CX-30", kind: "Car unit · SBC" },
];

// Artist page (Benjamin Francis Leftwich)
pub const ARTIST_NAME: &str = "Benjamin Francis Leftwich";
pub const ARTIST_STATS: &str = "3 ALBUMS · 34 TRACKS · 2 HR 14 MIN";
pub const ARTIST_ALBUMS: &[Album] = &[
    Album { n: "Last Smoke Before the Snowstorm", k: 12, y: "2011", art: "kind" },
    Album { n: "After the Rain", k: 10, y: "2016", art: "atlas" },
    Album { n: "Gratitude", k: 11, y: "2019", art: "cassette" },
];
pub struct TopSong {
    pub t: &'static str,
    pub al: &'static str,
    pub d: &'static str,
}
pub const ARTIST_TOP: &[TopSong] = &[
    TopSong { t: "Atlas Hands", al: "Last Smoke Before…", d: "4:32" },
    TopSong { t: "Box of Stones", al: "Last Smoke Before…", d: "3:58" },
    TopSong { t: "Tilikum", al: "After the Rain", d: "4:14" },
    TopSong { t: "Gratitude", al: "Gratitude", d: "3:47" },
];

// EQ — 10 bands, presets (dB per band)
pub const EQ_BANDS: [&str; 10] = ["32", "64", "125", "250", "500", "1k", "2k", "4k", "8k", "16k"];
// RAW half-dB units (see eq::BAND_MAX): the curves below were authored as decibels, so each value
// is doubled. ROCK's first band is +8 raw = +4 dB — which is what it always claimed to be and,
// until the units were measured on device, never was.
pub const EQ_PRESETS: [(&str, [i8; 10]); 5] = [
    ("FLAT", [0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
    ("ROCK", [8, 6, 2, 0, -2, 0, 4, 6, 8, 8]),
    ("JAZZ", [4, 2, 0, 2, 4, 2, 0, 2, 4, 6]),
    ("A1", [4, 6, 2, 0, -2, 0, 4, 6, 4, 2]),
    ("A2", [10, 8, 4, 0, 0, 2, 2, 4, 8, 10]),
];

// FM presets
pub const FM_PRESETS: [f32; 6] = [87.6, 88.6, 92.3, 96.1, 99.9, 104.7];
