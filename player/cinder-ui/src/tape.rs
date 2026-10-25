//! Custom cassette tapes for the Cassette style, in the format Wampy reads, so a tape folder made
//! for Wampy works here as it is: an 800×480 JPEG of the tape, a folder of reel frames shown in
//! turn every 55 ms while the song plays, and a `config.txt` placing the song's text and the reel.
//!
//! With a tape on the drive the Cassette style turns on its side and takes the whole panel
//! (`to_land`). Without one it is Cinder's drawn cassette. The shell finds and decodes the files
//! (`cinder-ffi/src/tape_load.rs`); this file holds what a tape is, how its `config.txt` reads, how
//! it draws, and what a tap on it does.
//!
//! Taps, as there is nothing on a tape to press: the left third is ◁, the middle ▶/‖, the right
//! third ▷, and the top right corner (where Wampy keeps its settings) opens the Menu.

use crate::art::Image;
use crate::canvas::Canvas;
use crate::now_playing::NowPlaying;
use crate::skin::{blit_to, hex_color};
use crate::text::{Family, FontSet, TextStyle, Weight};
use crate::theme::Theme;
use embedded_graphics::pixelcolor::Rgb888;

/// A reel frame lasts this long, as in Wampy.
pub const FRAME_MS: u32 = 55;

/// The panel on its side: the player's top on the left.
pub const LW: i32 = crate::canvas::H as i32;
pub const LH: i32 = crate::canvas::W as i32;

/// A touch at `(px, py)` on the panel, in the tape's landscape pixels.
pub fn to_land(px: i32, py: i32) -> (i32, i32) {
    (py, crate::canvas::W as i32 - 1 - px)
}

/// One text line on the tape: where it starts (a negative x hides it) and what it says.
#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    pub x: f32,
    pub y: f32,
    pub format: String,
}

/// A tape's `config.txt`. Every key is optional; these defaults are Wampy's.
#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub reel: String,
    pub artist: Field,
    pub title: Field,
    pub album: Field,
    pub reel_x: f32,
    pub reel_y: f32,
    pub title_width: f32,
    pub duration_format: String,
    pub text_color: u32,
}

impl Default for Config {
    fn default() -> Self {
        let field = |x, y, f: &str| Field {
            x,
            y,
            format: f.to_string(),
        };
        Config {
            reel: "other".into(),
            artist: field(83.0, 82.0, "$ARTIST"),
            title: field(83.0, 117.0, "$TITLE"),
            album: field(-1.0, -1.0, "$ALBUM"),
            reel_x: 134.0,
            reel_y: 160.0,
            title_width: 600.0,
            duration_format: "%1$02d:%2$02d".into(),
            text_color: 0x000000,
        }
    }
}

/// `config.txt`: `key: value` lines, `#` comments. Unknown keys and unreadable values are skipped.
pub fn parse_config(txt: &str) -> Config {
    let mut c = Config::default();
    for line in txt.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let v = v.trim();
        let num = v.parse::<f32>().ok();
        match (k.trim().to_ascii_lowercase().as_str(), num) {
            ("reel", _) => c.reel = v.to_string(),
            ("artistx", Some(n)) => c.artist.x = n,
            ("artisty", Some(n)) => c.artist.y = n,
            ("artistformat", _) => c.artist.format = v.to_string(),
            ("titlex", Some(n)) => c.title.x = n,
            ("titley", Some(n)) => c.title.y = n,
            ("titleformat", _) => c.title.format = v.to_string(),
            ("albumx", Some(n)) => c.album.x = n,
            ("albumy", Some(n)) => c.album.y = n,
            ("albumformat", _) => c.album.format = v.to_string(),
            ("reelx", Some(n)) => c.reel_x = n,
            ("reely", Some(n)) => c.reel_y = n,
            ("titlewidth", Some(n)) => c.title_width = n,
            ("durationformat", _) => c.duration_format = v.to_string(),
            ("textcolor", _) => c.text_color = hex_color(v).unwrap_or(c.text_color),
            _ => {}
        }
    }
    c
}

/// What a format line can name, for one song.
pub struct Song<'a> {
    pub title: &'a str,
    pub artist: &'a str,
    pub album: &'a str,
    pub year: &'a str,
    pub track: &'a str,
    /// Seconds.
    pub duration: u32,
}

/// `printf`'s `%N$0Wd` for the duration: argument 1 minutes, 2 seconds, 3 hours. Anything else
/// in the format is kept as written.
pub fn format_duration(fmt: &str, secs: u32) -> String {
    let args = [(secs / 60) % 60, secs % 60, secs / 3600];
    let mut out = String::new();
    let mut rest = fmt;
    while let Some(i) = rest.find('%') {
        out.push_str(&rest[..i]);
        let spec = &rest[i + 1..];
        let parsed = (|| {
            let (n, after) = spec.split_once('$')?;
            let n: usize = n.parse().ok()?;
            let end = after.find('d')?;
            let width: usize = after[..end].trim_start_matches('0').parse().unwrap_or(0);
            let pad0 = after.starts_with('0');
            let v = *args.get(n.checked_sub(1)?)?;
            let s = if pad0 {
                format!("{v:0width$}")
            } else {
                format!("{v:width$}")
            };
            Some((s, n.to_string().len() + 1 + end + 1))
        })();
        match parsed {
            Some((s, used)) => {
                out.push_str(&s);
                rest = &spec[used..];
            }
            None => {
                out.push('%');
                rest = spec;
            }
        }
    }
    out.push_str(rest);
    out
}

/// `"4:21"` or `"1:02:03"` in seconds; 0 when it is not a time.
pub fn secs(dur: &str) -> u32 {
    dur.split(':')
        .try_fold(0u32, |acc, p| {
            p.trim().parse::<u32>().ok().map(|n| acc * 60 + n)
        })
        .unwrap_or(0)
}

/// A format line filled in: `$title` as written, `$TITLE` in capitals, and the same for the rest.
pub fn fill(fmt: &str, s: &Song, duration_format: &str) -> String {
    let dur = format_duration(duration_format, s.duration);
    let mut out = fmt.to_string();
    for (name, v) in [
        ("title", s.title),
        ("artist", s.artist),
        ("album", s.album),
        ("year", s.year),
        ("track", s.track),
        ("duration", dur.as_str()),
    ] {
        out = out.replace(&format!("${}", name.to_uppercase()), &v.to_uppercase());
        out = out.replace(&format!("${name}"), v);
    }
    out
}

/// A tape the shell has decoded.
pub struct Tape {
    /// The tape, scaled to the panel on its side (800×480).
    pub image: Image,
    /// The reel's frames, in name order. Empty: no reel drawn.
    pub reel: Vec<Image>,
    pub config: Config,
    /// The folder's name, for the line that says which tape is in use.
    pub name: String,
}

/// What a tap on a tape does.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tap {
    Menu,
    Prev,
    PlayPause,
    Next,
}

/// The tap at `(lx, ly)`, in landscape pixels.
pub fn tap_at(lx: i32, ly: i32) -> Tap {
    if lx >= LW - 120 && ly < 100 {
        return Tap::Menu;
    }
    match lx * 3 / LW {
        0 => Tap::Prev,
        1 => Tap::PlayPause,
        _ => Tap::Next,
    }
}

/// Draw `tape` across the panel. `ms`: a clock that runs while the song plays, for the reel.
pub fn render(
    c: &mut Canvas,
    t: &Theme,
    f: &FontSet,
    np: &NowPlaying,
    tape: &Tape,
    song: &Song,
    ms: u32,
) {
    let dim = t.dim_pct;
    let img = &tape.image;
    blit_to(
        c,
        dim,
        img,
        (0, 0, img.w as i32, img.h as i32),
        (0, 0, LW, LH),
        true,
        None,
    );
    if !tape.reel.is_empty() {
        let n = (ms / FRAME_MS) as usize % tape.reel.len();
        let fr = &tape.reel[n];
        let (x, y) = (tape.config.reel_x as i32, tape.config.reel_y as i32);
        let src = (0, 0, fr.w as i32, fr.h as i32);
        blit_to(
            c,
            dim,
            fr,
            src,
            (x, y, fr.w as i32, fr.h as i32),
            true,
            None,
        );
    }
    if np.title.is_empty() && np.artist.is_empty() {
        return;
    }
    let col = tape.config.text_color;
    let ch = |s: u32| (((col >> s) & 0xff) * dim.min(100) / 100) as u8;
    let st = TextStyle {
        fam: Family::Sans,
        weight: Weight::SemiBold,
        size: 22.0,
        color: Rgb888::new(ch(16), ch(8), ch(0)),
        tracking: 0.0,
    };
    let cfg = &tape.config;
    for field in [&cfg.artist, &cfg.title, &cfg.album] {
        if field.x < 0.0 {
            continue;
        }
        let line = fill(&field.format, song, &cfg.duration_format);
        // Wampy's y is the line's top; the text is drawn from its baseline.
        let base = field.y + 22.0;
        crate::text::draw_land(c, f, (field.x, base), &line, &st, cfg.title_width);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_reads_wampys_keys_and_keeps_defaults() {
        let c = parse_config(
            "# default reel\nreel: other2\ntitlex: 10\ntitley: -1\nalbumx: 5.5\ntextcolor: #FF8000\nnonsense: 3\nreelx: x\n",
        );
        assert_eq!(c.reel, "other2");
        assert_eq!((c.title.x, c.title.y), (10.0, -1.0));
        assert_eq!(c.album.x, 5.5);
        assert_eq!(c.text_color, 0xff8000);
        assert_eq!(c.reel_x, 134.0, "an unreadable value keeps the default");
        assert_eq!(c.artist, Config::default().artist);
    }

    #[test]
    fn formats_fill_in_like_wampys() {
        assert_eq!(format_duration("%1$02d:%2$02d", 3 * 60 + 7), "03:07");
        assert_eq!(
            format_duration("%3$01d:%1$02d:%2$02d", 2 * 3600 + 6 * 60 + 37),
            "2:06:37"
        );
        assert_eq!(format_duration("100%", 5), "100%", "a lone % is kept");
        assert_eq!((secs("4:21"), secs("1:02:03"), secs("")), (261, 3723, 0));
        let s = Song {
            title: "Night Train",
            artist: "The Example Band",
            album: "The Example Band",
            year: "1977",
            track: "1",
            duration: 150,
        };
        assert_eq!(fill("$ARTIST", &s, "%1$02d:%2$02d"), "THE EXAMPLE BAND");
        assert_eq!(
            fill("$track. $title ($duration, $year)", &s, "%1$02d:%2$02d"),
            "1. Night Train (02:30, 1977)"
        );
    }

    #[test]
    fn taps_split_the_tape_in_three_with_the_menu_in_the_corner() {
        assert_eq!(tap_at(10, 300), Tap::Prev);
        assert_eq!(tap_at(400, 300), Tap::PlayPause);
        assert_eq!(tap_at(790, 300), Tap::Next);
        assert_eq!(tap_at(790, 20), Tap::Menu);
    }
}
