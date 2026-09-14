//! One alphabetical order for the whole Library — every list, and the A–Z rail's letters.
//!
//! Lists used to sort with `str::cmp`, which is BYTE order: every capital sorts before every
//! lowercase letter, and every accented letter after `z`. On the reference library (2026-09-15)
//! that put 12 of 310 artists — `alt‐J`, `bôa`, `julie`, `the north` … — below `Zola Jesus`, and
//! 127 song titles with them, while the rail filed the same names under their letters. Sony's own
//! scanner files them under their letters too: its `artists.initial` column has `alt‐J` under A,
//! `bôa` under B and `the north` under N.
//!
//! The order, and nothing more:
//!   1. in ARTIST names, and only when the user asks for it (Settings ▸ Ignore "The" in artists),
//!      a leading whole-word "The" in any case is skipped — "The Beatles" among the B's, "Theatre
//!      of Tragedy" still under T. Titles, album names, folders and playlists keep it;
//!   2. names that start with a digit or punctuation come first — the rail's `#`, drawn at the
//!      top — then Latin letters, then every other script. Sony's `initial` column puts kana and
//!      kanji after Z as well, so that part of the old order was already right;
//!   3. within a group, letters compare case-blind with accents folded: `bôa` beside `Boa`,
//!      `Édith` among the E's, `ß` as `ss`, `æ` as `ae`. Folding covers Latin-1 Supplement and
//!      Latin Extended-A; a letter outside those sorts with the other scripts;
//!   4. exact ties fall back to byte order, so the sort is total and the same on every run.
//!
//! Allocation-free on purpose: `library::song_order` sorts the whole Songs list every frame the
//! tab is drawn.

use std::cmp::Ordering;

/// Compare two names in the Library's alphabetical order: titles, album names, folders,
/// playlists — anything where "The" is part of the name. Artist names go through [`cmp_artist`].
pub fn cmp(a: &str, b: &str) -> Ordering {
    cmp_with(a.trim(), b.trim(), a, b)
}

/// Compare two ARTIST names. With `ignore_the` — the user's setting — a leading whole-word "The"
/// is skipped on both sides; without it this is exactly [`cmp`].
pub fn cmp_artist(a: &str, b: &str, ignore_the: bool) -> Ordering {
    if ignore_the {
        cmp_with(strip_article(a), strip_article(b), a, b)
    } else {
        cmp(a, b)
    }
}

/// Group, then folded characters, then bytes of the ORIGINAL names as the final tie-break.
fn cmp_with(x: &str, y: &str, a: &str, b: &str) -> Ordering {
    group(x)
        .cmp(&group(y))
        .then_with(|| Folded::new(x).cmp(Folded::new(y)))
        .then_with(|| a.cmp(b))
}

/// The rail letter a name files under: its first folded letter in upper case, or `#` when it does
/// not start with a Latin letter. Sorting by [`cmp`] never makes these letters go backwards.
pub fn initial(s: &str) -> u8 {
    initial_of(s.trim())
}

/// The rail letter an ARTIST name files under, skipping "The" exactly when [`cmp_artist`] does.
pub fn artist_initial(s: &str, ignore_the: bool) -> u8 {
    initial_of(if ignore_the { strip_article(s) } else { s.trim() })
}

fn initial_of(s: &str) -> u8 {
    match Folded::new(s).next() {
        Some(c) if c.is_ascii_lowercase() => c.to_ascii_uppercase() as u8,
        _ => b'#',
    }
}

/// `s` trimmed, without a leading whole-word "The " in any case. A name that is only "The" keeps it.
fn strip_article(s: &str) -> &str {
    let t = s.trim();
    match t.get(..4) {
        Some(head) if head.eq_ignore_ascii_case("the ") => {
            let rest = t[4..].trim_start();
            if rest.is_empty() { t } else { rest }
        }
        _ => t,
    }
}

/// 0 = starts with a digit, punctuation, a symbol, or nothing; 1 = a Latin letter; 2 = a letter
/// or digit of any other script.
fn group(s: &str) -> u8 {
    match Folded::new(s).next() {
        Some(c) if c.is_ascii_lowercase() => 1,
        Some(c) if c.is_alphanumeric() && !c.is_ascii_digit() => 2,
        _ => 0,
    }
}

/// The characters of a name as the collation compares them: ASCII lower-cased, Latin-1 and
/// Latin Extended-A letters folded to their ASCII letter (or two, for `ß`, `æ`, `œ`, `þ`, `ĳ`),
/// and anything else lower-cased as Unicode defines it.
struct Folded<'a> {
    chars: std::str::Chars<'a>,
    pending: Option<char>,
}

impl<'a> Folded<'a> {
    fn new(s: &'a str) -> Self {
        Folded { chars: s.chars(), pending: None }
    }
}

impl Iterator for Folded<'_> {
    type Item = char;

    fn next(&mut self) -> Option<char> {
        if let Some(c) = self.pending.take() {
            return Some(c);
        }
        let c = self.chars.next()?;
        if c.is_ascii() {
            return Some(c.to_ascii_lowercase());
        }
        if let Some(fold) = (c as usize).checked_sub(0xC0).and_then(|i| LATIN.get(i)) {
            match *fold {
                [one] => return Some(*one as char),
                [one, two] => {
                    self.pending = Some(*two as char);
                    return Some(*one as char);
                }
                _ => {} // × and ÷ are not letters: they compare as themselves
            }
        }
        Some(c.to_lowercase().next().unwrap_or(c))
    }
}

/// `LATIN[c - 0xC0]` = the lowercase ASCII letter(s) a Latin-1 Supplement or Latin Extended-A
/// character folds to, or `b""` for the two that are not letters (`×` U+00D7, `÷` U+00F7).
/// Generated from Unicode's canonical decompositions, plus the letters that have none (æ ð ø þ ß
/// đ ħ ı ĳ ĸ ŀ ł ŉ ŋ œ ŧ ſ), which fold the way a reader spells them.
const LATIN: [&[u8]; 0xC0] = [
    b"a", b"a", b"a", b"a", b"a", b"a", b"ae", b"c", b"e", b"e", b"e", b"e", b"i", b"i", b"i",
    b"i", b"d", b"n", b"o", b"o", b"o", b"o", b"o", b"", b"o", b"u", b"u", b"u", b"u", b"y", b"th",
    b"ss", b"a", b"a", b"a", b"a", b"a", b"a", b"ae", b"c", b"e", b"e", b"e", b"e", b"i", b"i",
    b"i", b"i", b"d", b"n", b"o", b"o", b"o", b"o", b"o", b"", b"o", b"u", b"u", b"u", b"u", b"y",
    b"th", b"y", b"a", b"a", b"a", b"a", b"a", b"a", b"c", b"c", b"c", b"c", b"c", b"c", b"c",
    b"c", b"d", b"d", b"d", b"d", b"e", b"e", b"e", b"e", b"e", b"e", b"e", b"e", b"e", b"e", b"g",
    b"g", b"g", b"g", b"g", b"g", b"g", b"g", b"h", b"h", b"h", b"h", b"i", b"i", b"i", b"i", b"i",
    b"i", b"i", b"i", b"i", b"i", b"ij", b"ij", b"j", b"j", b"k", b"k", b"k", b"l", b"l", b"l",
    b"l", b"l", b"l", b"l", b"l", b"l", b"l", b"n", b"n", b"n", b"n", b"n", b"n", b"n", b"n", b"n",
    b"o", b"o", b"o", b"o", b"o", b"o", b"oe", b"oe", b"r", b"r", b"r", b"r", b"r", b"r", b"s",
    b"s", b"s", b"s", b"s", b"s", b"s", b"s", b"t", b"t", b"t", b"t", b"t", b"t", b"u", b"u", b"u",
    b"u", b"u", b"u", b"u", b"u", b"u", b"u", b"u", b"u", b"w", b"w", b"y", b"y", b"y", b"z", b"z",
    b"z", b"z", b"z", b"z", b"s",
];

#[cfg(test)]
mod tests {
    use super::*;

    /// The artists that sat below "Z" on the reference library (2026-09-15), shuffled in among the
    /// neighbours they belong between — sorted both ways the setting allows.
    #[test]
    fn the_misfiled_artists_sort_where_they_are_filed() {
        let shuffled = [
            "widowdusk", "高中正義", "the north", "Zola Jesus", "bôa", "112", "julie", "Björk",
            "アルク", "alt‐J", "Julie", "The Beatles", "Édith Piaf", "Boa", "A Tribe Called Quest",
            "'Round About Midnight",
        ];
        let mut as_written = shuffled.to_vec();
        as_written.sort_by(|a, b| cmp_artist(a, b, false));
        assert_eq!(
            as_written,
            [
                "'Round About Midnight", "112", "A Tribe Called Quest", "alt‐J", "Björk", "Boa",
                "bôa", "Édith Piaf", "Julie", "julie", "The Beatles", "the north", "widowdusk",
                "Zola Jesus", "アルク", "高中正義",
            ]
        );
        let mut ignoring_the = shuffled.to_vec();
        ignoring_the.sort_by(|a, b| cmp_artist(a, b, true));
        assert_eq!(
            ignoring_the,
            [
                "'Round About Midnight", "112", "A Tribe Called Quest", "alt‐J", "The Beatles",
                "Björk", "Boa", "bôa", "Édith Piaf", "Julie", "julie", "the north", "widowdusk",
                "Zola Jesus", "アルク", "高中正義",
            ]
        );
    }

    #[test]
    fn a_leading_the_is_one_whole_word_in_any_case() {
        assert_eq!(strip_article("THE NORTH"), "NORTH");
        assert_eq!(strip_article("  the   xx "), "xx");
        assert_eq!(strip_article("Theatre of Tragedy"), "Theatre of Tragedy");
        assert_eq!(strip_article("The"), "The");
        assert_eq!(cmp_artist("The Zombies", "Theatre of Tragedy", true), Ordering::Greater);
        assert_eq!(cmp_artist("The Zombies", "Theatre of Tragedy", false), Ordering::Less);
        assert_eq!(cmp("The Zombies", "Zebra"), Ordering::Less, "titles keep their The");
    }

    #[test]
    fn special_letters_fold_the_way_they_are_spelled() {
        assert_eq!(Folded::new("Straße Æon Øyvind Łódź").collect::<String>(), "strasse aeon oyvind lodz");
        assert_eq!(cmp("Øyvind", "Oz"), Ordering::Less);
        assert_eq!(cmp("a × b", "a x b"), "a × b".cmp("a x b"), "× is not an x");
    }

    #[test]
    fn initial_follows_the_folding() {
        assert_eq!(initial("bôa"), b'B');
        assert_eq!(initial("Édith Piaf"), b'E');
        assert_eq!(initial("the north"), b'T');
        assert_eq!(artist_initial("the north", false), b'T');
        assert_eq!(artist_initial("the north", true), b'N');
        assert_eq!(initial("112"), b'#');
        assert_eq!(initial("...And Justice For All"), b'#');
        assert_eq!(initial("アルク"), b'#');
        assert_eq!(initial(""), b'#');
    }

    #[test]
    fn every_latin1_and_extended_a_letter_folds_to_ascii_letters() {
        for cp in 0xC0..0x180u32 {
            let ch = char::from_u32(cp).unwrap();
            let fold = LATIN[(cp - 0xC0) as usize];
            if ch.is_alphabetic() {
                assert!(!fold.is_empty() && fold.iter().all(u8::is_ascii_lowercase), "{ch} U+{cp:04X}");
            } else {
                assert!(fold.is_empty(), "{ch} U+{cp:04X} is not a letter");
            }
        }
    }
}
