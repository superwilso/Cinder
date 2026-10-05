//! Saved views (design handoff 5c) and the smart playlists they appear as (5g).
//!
//! A saved view is a NAME plus rules and a sort: "four stars and up, played this month, FLAC,
//! most played first". The handoff also gives one a "show as" and a "pin to the Library bar"; both
//! belong to the Library view bar, which is R3 and not built, so this model holds only what does
//! something today (`docs/PLAN_redesign_2026-09.md`, Part G).
//!
//! Today a view appears in exactly one place: the Library's Playlists tab, as a SMART playlist
//! above the ordinary ones, whose members are whatever the rules match right now. Nothing about the
//! members is stored — a view is the question, not the answer — so a rating given or a song played
//! is reflected the next time the list is built.
//!
//! The file is `/contents/cinder_views.conf`, Cinder's own, written by the shell with a temporary
//! file and a rename. The format is in `docs/TRACK_DATA.md`; [`parse`] and [`serialize`] are the
//! one implementation of it.

use crate::model::{Format, Library, SongRow, TrackStat};

/// More than anyone will make on a player with a 3-inch screen, and small enough that a corrupt
/// file cannot fill the Playlists tab.
pub const MAX_VIEWS: usize = 32;

/// Seconds in a day. `i64`, like every time here: a play date must not wrap in 2038.
const DAY: i64 = 86_400;
/// "Recently" means the last 30 days.
pub const RECENT_DAYS: i64 = 30;
/// "Not lately" means played before, but not in the last 90 days.
pub const STALE_DAYS: i64 = 90;

/// The rating rule's steps, as minimum stars (0 = any), and their chip labels.
pub const RATINGS: [u8; 5] = [0, 2, 3, 4, 5];
pub const RATING_LABELS: [&str; 5] = ["Any", "2+", "3+", "4+", "5"];

/// When the track was last played.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Played {
    #[default]
    Any,
    /// In the last [`RECENT_DAYS`].
    Recent,
    /// Played at some point, but not in the last [`STALE_DAYS`] — the songs you have forgotten.
    NotLately,
    /// Never counted as played on this player.
    Never,
}

impl Played {
    pub const ALL: [Played; 4] = [
        Played::Any,
        Played::Recent,
        Played::NotLately,
        Played::Never,
    ];
    pub const LABELS: [&'static str; 4] = ["Any", "Recent", "Not lately", "Never"];
    fn token(self) -> &'static str {
        match self {
            Played::Any => "any",
            Played::Recent => "recent",
            Played::NotLately => "not_lately",
            Played::Never => "never",
        }
    }
}

/// The container rule. Hi-Res is not a container, but it is the question people ask of one, and
/// the library already carries the flag.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FormatRule {
    #[default]
    Any,
    Flac,
    Mp3,
    M4a,
    HiRes,
}

impl FormatRule {
    pub const ALL: [FormatRule; 5] = [
        FormatRule::Any,
        FormatRule::Flac,
        FormatRule::Mp3,
        FormatRule::M4a,
        FormatRule::HiRes,
    ];
    pub const LABELS: [&'static str; 5] = ["Any", "FLAC", "MP3", "M4A", "Hi-Res"];
    fn token(self) -> &'static str {
        match self {
            FormatRule::Any => "any",
            FormatRule::Flac => "flac",
            FormatRule::Mp3 => "mp3",
            FormatRule::M4a => "m4a",
            FormatRule::HiRes => "hires",
        }
    }
}

/// The order a view's songs come in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ViewSort {
    /// A to Z by title, in the Library's collation.
    #[default]
    Title,
    /// Most played first.
    Plays,
    /// Most recently played first; never-played last.
    Played,
    /// Highest rated first.
    Rating,
    /// Newest in the library first.
    Added,
}

impl ViewSort {
    pub const ALL: [ViewSort; 5] = [
        ViewSort::Title,
        ViewSort::Plays,
        ViewSort::Played,
        ViewSort::Rating,
        ViewSort::Added,
    ];
    pub const LABELS: [&'static str; 5] = ["Title", "Plays", "Played", "Rating", "Added"];
    fn token(self) -> &'static str {
        match self {
            ViewSort::Title => "title",
            ViewSort::Plays => "plays",
            ViewSort::Played => "played",
            ViewSort::Rating => "rating",
            ViewSort::Added => "added",
        }
    }
}

fn from_token<T: Copy + Default>(all: &[T], token: impl Fn(T) -> &'static str, s: &str) -> T {
    all.iter()
        .copied()
        .find(|v| token(*v) == s.trim())
        .unwrap_or_default()
}

/// One saved view.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SavedView {
    pub name: String,
    /// Minimum stars; 0 = any rating, including none.
    pub min_rating: u8,
    pub played: Played,
    pub format: FormatRule,
    pub sort: ViewSort,
    /// What this playlist's Shuffle band deals: songs, whole albums or whole artists. `None`
    /// follows Settings ▸ Shuffle, which is what every view did before 2026-10-04.
    pub shuffle: Option<crate::shuffle::ShuffleBy>,
}

/// The editor's SHUFFLE chips: "Settings" (follow the setting), then [`ShuffleBy::ALL`].
pub const SHUFFLE_LABELS: [&str; 4] = ["Settings", "Songs", "Albums", "Artists"];

/// `shuffle=` in `cinder_views.conf`.
fn shuffle_token(s: Option<crate::shuffle::ShuffleBy>) -> &'static str {
    s.map_or("settings", crate::shuffle::ShuffleBy::token)
}

fn shuffle_from_token(s: &str) -> Option<crate::shuffle::ShuffleBy> {
    crate::shuffle::ShuffleBy::ALL
        .into_iter()
        .find(|b| b.token() == s.trim())
}

/// The bit that marks a playlist id as a smart one. Sony's playlist ids are SQLite row ids —
/// small positive numbers — and Cinder's own `.m3u8` lists are negative, so the top of the
/// positive range is free.
pub const SMART_ID_BIT: i64 = 1 << 62;

/// The playlist id a view draws as. From the NAME (case-blind), so it is stable across reboots and
/// across edits that keep the name — which is what lets the page stay open while its rules change.
pub fn smart_id(name: &str) -> i64 {
    SMART_ID_BIT | (crate::shuffle::artist_key(name) & (SMART_ID_BIT as u64 - 1)) as i64
}

/// Is this a smart playlist's id?
pub fn is_smart_id(id: i64) -> bool {
    id >= SMART_ID_BIT
}

impl SavedView {
    /// A new view as the editor opens it: every rule at Any, sorted by title.
    pub fn new(name: &str) -> SavedView {
        SavedView {
            name: name.to_string(),
            ..Default::default()
        }
    }

    pub fn id(&self) -> i64 {
        smart_id(&self.name)
    }

    /// Does `s` (with its stats) pass every rule? `now` is the wall clock in unix seconds; `<= 0`
    /// means the clock is not known yet, and then "Recent" accepts anything ever played and "Not
    /// lately" nothing — the two answers that do not depend on a date we do not have.
    pub fn matches(&self, s: &SongRow, st: TrackStat, now: i64) -> bool {
        if st.rating < self.min_rating {
            return false;
        }
        let ok_played = match self.played {
            Played::Any => true,
            Played::Never => st.plays == 0,
            Played::Recent => {
                st.last_played > 0 && (now <= 0 || now - st.last_played <= RECENT_DAYS * DAY)
            }
            Played::NotLately => st.plays > 0 && now > 0 && now - st.last_played > STALE_DAYS * DAY,
        };
        let ok_format = match self.format {
            FormatRule::Any => true,
            FormatRule::Flac => s.format == Format::Flac,
            FormatRule::Mp3 => s.format == Format::Mp3,
            FormatRule::M4a => s.format == Format::M4a,
            FormatRule::HiRes => s.is_hires,
        };
        ok_played && ok_format
    }

    /// The songs this view holds, as indices into `lib.songs`, in the view's order. Ties fall back
    /// to title, so the list is the same list on every build.
    pub fn tracks(&self, lib: &Library, now: i64) -> Vec<usize> {
        let mut out: Vec<usize> = lib
            .songs
            .iter()
            .enumerate()
            .filter(|(_, s)| self.matches(s, lib.stat(s.object_id), now))
            .map(|(i, _)| i)
            .collect();
        let s = &lib.songs;
        let ranks = lib.ranks();
        let by_title = |a: usize, b: usize| match ranks {
            Some(r) => r.title(a).cmp(&r.title(b)),
            None => crate::collate::cmp(&s[a].title, &s[b].title),
        };
        let st = |i: usize| lib.stat(s[i].object_id);
        match self.sort {
            ViewSort::Title => out.sort_by(|&a, &b| by_title(a, b)),
            ViewSort::Plays => {
                out.sort_by(|&a, &b| st(b).plays.cmp(&st(a).plays).then_with(|| by_title(a, b)))
            }
            ViewSort::Played => out.sort_by(|&a, &b| {
                st(b)
                    .last_played
                    .cmp(&st(a).last_played)
                    .then_with(|| by_title(a, b))
            }),
            ViewSort::Rating => {
                out.sort_by(|&a, &b| st(b).rating.cmp(&st(a).rating).then_with(|| by_title(a, b)))
            }
            ViewSort::Added => {
                out.sort_by(|&a, &b| s[b].added.cmp(&s[a].added).then_with(|| by_title(a, b)))
            }
        }
        out
    }

    /// One line saying what the rules are, for the playlist row and the page: "4+ stars · Recent ·
    /// FLAC". "Every song" when no rule is set.
    pub fn summary(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if self.min_rating > 0 {
            parts.push(if self.min_rating >= 5 {
                "5 stars".to_string()
            } else {
                format!("{}+ stars", self.min_rating)
            });
        }
        if self.played != Played::Any {
            parts.push(
                Played::LABELS[Played::ALL
                    .iter()
                    .position(|p| *p == self.played)
                    .unwrap_or(0)]
                .to_string(),
            );
        }
        if self.format != FormatRule::Any {
            parts.push(
                FormatRule::LABELS[FormatRule::ALL
                    .iter()
                    .position(|p| *p == self.format)
                    .unwrap_or(0)]
                .to_string(),
            );
        }
        if parts.is_empty() {
            "Every song".to_string()
        } else {
            parts.join(" \u{b7} ")
        }
    }
}

/// A name fit for the file: no line breaks, no brackets (they delimit the section), trimmed, at
/// most 48 characters. Empty stays empty — the caller decides what an unnamed view is called.
pub fn clean_name(name: &str) -> String {
    let s: String = name
        .chars()
        .filter(|c| !c.is_control() && *c != '[' && *c != ']')
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    s.chars().take(48).collect()
}

/// `name`, or `name 2`, `name 3`… — the first spelling no OTHER view has (case-blind). `skip` is
/// the index of the view being renamed, which may keep its own name.
pub fn unique_name(views: &[SavedView], name: &str, skip: Option<usize>) -> String {
    let base = if name.trim().is_empty() {
        "Smart playlist".to_string()
    } else {
        name.to_string()
    };
    let taken = |n: &str| {
        views
            .iter()
            .enumerate()
            .any(|(i, v)| Some(i) != skip && v.name.to_lowercase() == n.to_lowercase())
    };
    if !taken(&base) {
        return base;
    }
    (2..1000)
        .map(|k| format!("{base} {k}"))
        .find(|n| !taken(n))
        .unwrap_or(base)
}

/// Read `cinder_views.conf`. Tolerant the way every config reader here is: an unknown key or a bad
/// value falls back to its default, a line outside any section is ignored, and a section with an
/// empty name is dropped — a hand-edited file must never keep the Playlists tab from drawing.
pub fn parse(body: &str) -> Vec<SavedView> {
    let mut out: Vec<SavedView> = Vec::new();
    for line in body.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        if let Some(rest) = l.strip_prefix('[') {
            let name = clean_name(rest.rsplit_once(']').map_or(rest, |(n, _)| n));
            if !name.is_empty() && out.len() < MAX_VIEWS {
                let name = unique_name(&out, &name, None);
                out.push(SavedView::new(&name));
            } else {
                // A section we will not keep: stop its keys landing on the previous view.
                out.push(SavedView::default());
            }
            continue;
        }
        let Some(v) = out.last_mut() else { continue };
        let Some((k, val)) = l.split_once('=') else {
            continue;
        };
        match k.trim() {
            "rating" => v.min_rating = val.trim().parse::<u8>().unwrap_or(0).min(5),
            "played" => v.played = from_token(&Played::ALL, Played::token, val),
            "format" => v.format = from_token(&FormatRule::ALL, FormatRule::token, val),
            "sort" => v.sort = from_token(&ViewSort::ALL, ViewSort::token, val),
            "shuffle" => v.shuffle = shuffle_from_token(val),
            _ => {}
        }
    }
    out.retain(|v| !v.name.is_empty());
    out
}

/// Write `cinder_views.conf`. Every key is written, defaults included, so a person reading the
/// file sees every rule a view has rather than having to know which ones were left out.
pub fn serialize(views: &[SavedView]) -> String {
    let mut s = String::from(
        "# Cinder saved views (smart playlists). Written by the player; see docs/TRACK_DATA.md.\n",
    );
    for v in views {
        s.push_str(&format!(
            "\n[{}]\nrating={}\nplayed={}\nformat={}\nsort={}\nshuffle={}\n",
            clean_name(&v.name),
            v.min_rating,
            v.played.token(),
            v.format.token(),
            v.sort.token(),
            shuffle_token(v.shuffle)
        ));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-10-04: a view remembers what its Shuffle band deals. A file from before the key
    /// existed, or one with a word nobody knows, follows the setting.
    #[test]
    fn a_views_shuffle_mode_round_trips_and_defaults_to_the_setting() {
        use crate::shuffle::ShuffleBy;
        let mut v = SavedView::new("Late");
        assert_eq!(v.shuffle, None);
        v.shuffle = Some(ShuffleBy::Albums);
        let body = serialize(&[v.clone(), SavedView::new("Plain")]);
        assert!(
            body.contains("shuffle=albums") && body.contains("shuffle=settings"),
            "{body}"
        );
        let back = parse(&body);
        assert_eq!(back[0], v);
        assert_eq!(back[1].shuffle, None);
        assert_eq!(
            parse("[Old]\nrating=3\n")[0].shuffle,
            None,
            "a file from before the key"
        );
        assert_eq!(parse("[Odd]\nshuffle=sideways\n")[0].shuffle, None);
        assert_eq!(
            parse("[Songs]\nshuffle=songs\n")[0].shuffle,
            Some(ShuffleBy::Songs)
        );
    }

    fn song(title: &str, id: i64, format: Format, hires: bool, added: i64) -> SongRow {
        SongRow {
            title: title.into(),
            object_id: id,
            format,
            is_hires: hires,
            added,
            ..Default::default()
        }
    }

    fn lib() -> Library {
        let mut l = Library {
            songs: vec![
                song("Alpha", 1, Format::Flac, true, 10),
                song("Bravo", 2, Format::Mp3, false, 30),
                song("Charlie", 3, Format::Flac, false, 20),
                song("Delta", 4, Format::M4a, false, 40),
            ],
            ..Default::default()
        };
        let now = 1_000 * DAY;
        l.stats.insert(
            1,
            TrackStat {
                rating: 5,
                plays: 9,
                last_played: now - DAY,
            },
        );
        l.stats.insert(
            2,
            TrackStat {
                rating: 3,
                plays: 2,
                last_played: now - 200 * DAY,
            },
        );
        l.stats.insert(
            3,
            TrackStat {
                rating: 4,
                plays: 0,
                last_played: 0,
            },
        );
        l
    }
    const NOW: i64 = 1_000 * DAY;

    fn titles(l: &Library, v: &SavedView) -> Vec<String> {
        v.tracks(l, NOW)
            .iter()
            .map(|&i| l.songs[i].title.clone())
            .collect()
    }

    #[test]
    fn each_rule_filters_on_its_own() {
        let l = lib();
        let v = |f: fn(&mut SavedView)| {
            let mut v = SavedView::new("x");
            f(&mut v);
            v
        };
        assert_eq!(
            titles(&l, &v(|_| {})),
            ["Alpha", "Bravo", "Charlie", "Delta"],
            "no rules: every song"
        );
        assert_eq!(titles(&l, &v(|v| v.min_rating = 4)), ["Alpha", "Charlie"]);
        assert_eq!(titles(&l, &v(|v| v.played = Played::Recent)), ["Alpha"]);
        assert_eq!(titles(&l, &v(|v| v.played = Played::NotLately)), ["Bravo"]);
        assert_eq!(
            titles(&l, &v(|v| v.played = Played::Never)),
            ["Charlie", "Delta"]
        );
        assert_eq!(
            titles(&l, &v(|v| v.format = FormatRule::Flac)),
            ["Alpha", "Charlie"]
        );
        assert_eq!(titles(&l, &v(|v| v.format = FormatRule::HiRes)), ["Alpha"]);
        assert_eq!(titles(&l, &v(|v| v.format = FormatRule::M4a)), ["Delta"]);
        // Rules AND together.
        assert_eq!(
            titles(
                &l,
                &v(|v| {
                    v.min_rating = 3;
                    v.format = FormatRule::Flac;
                })
            ),
            ["Alpha", "Charlie"]
        );
    }

    #[test]
    fn each_sort_orders_as_its_label_says() {
        let l = lib();
        let sorted = |s: ViewSort| {
            titles(
                &l,
                &SavedView {
                    sort: s,
                    ..SavedView::new("x")
                },
            )
        };
        assert_eq!(
            sorted(ViewSort::Title),
            ["Alpha", "Bravo", "Charlie", "Delta"]
        );
        assert_eq!(
            sorted(ViewSort::Plays),
            ["Alpha", "Bravo", "Charlie", "Delta"]
        );
        assert_eq!(
            sorted(ViewSort::Played),
            ["Alpha", "Bravo", "Charlie", "Delta"]
        );
        assert_eq!(
            sorted(ViewSort::Rating),
            ["Alpha", "Charlie", "Bravo", "Delta"]
        );
        assert_eq!(
            sorted(ViewSort::Added),
            ["Delta", "Bravo", "Charlie", "Alpha"]
        );
    }

    /// Before the shell has reported the clock, a date rule must not invent an answer.
    #[test]
    fn an_unknown_clock_does_not_guess_ages() {
        let l = lib();
        let recent = SavedView {
            played: Played::Recent,
            ..SavedView::new("x")
        };
        let stale = SavedView {
            played: Played::NotLately,
            ..SavedView::new("x")
        };
        let n = |v: &SavedView| v.tracks(&l, 0).len();
        assert_eq!(n(&recent), 2, "anything ever played");
        assert_eq!(n(&stale), 0, "nothing");
    }

    #[test]
    fn the_file_round_trips_and_forgives_junk() {
        let views = vec![
            SavedView {
                name: "Late favourites".into(),
                min_rating: 4,
                played: Played::Recent,
                format: FormatRule::Flac,
                sort: ViewSort::Plays,
                shuffle: None,
            },
            SavedView::new("Everything"),
        ];
        assert_eq!(parse(&serialize(&views)), views);
        let junk = "stray=1\n[Good]\nrating=9\nplayed=sometimes\nsort=plays\nwho=knows\n[]\nrating=2\n[good]\n";
        let got = parse(junk);
        assert_eq!(got.len(), 2, "{got:?}");
        assert_eq!(got[0].min_rating, 5, "clamped");
        assert_eq!(got[0].played, Played::Any, "an unknown word is the default");
        assert_eq!(got[0].sort, ViewSort::Plays);
        assert_eq!(got[1].name, "good 2", "names stay unique, so ids do");
    }

    #[test]
    fn smart_ids_are_stable_and_clear_of_every_other_kind() {
        assert_eq!(smart_id("Mix"), smart_id("mix"));
        assert_ne!(smart_id("Mix"), smart_id("Mix 2"));
        assert!(is_smart_id(smart_id("anything")));
        assert!(!is_smart_id(12_345), "a Sony playlist");
        assert!(!is_smart_id(-12_345), "one of Cinder's .m3u8 lists");
    }

    #[test]
    fn the_summary_names_the_rules() {
        assert_eq!(SavedView::new("x").summary(), "Every song");
        let v = SavedView {
            min_rating: 4,
            played: Played::NotLately,
            format: FormatRule::HiRes,
            ..SavedView::new("x")
        };
        assert_eq!(v.summary(), "4+ stars \u{b7} Not lately \u{b7} Hi-Res");
    }
}
