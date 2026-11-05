//! Settings ▸ Wi-Fi ▸ Internet radio: stations from `cinder_radio.txt` on the drive, played by the
//! shell (cinder-home) through the headphone jack or Bluetooth. Part of the Wi-Fi install option;
//! nothing here does any I/O — the shell hands in the file's text and the player's status.
//!
//!   ┌ Internet radio ──────────────────┐
//!   │ PLAYING · Groove Salad · Artist – Song │  the strip: what the shell says
//!   │ STATIONS                         │
//!   │ Groove Salad              PLAYING │  tap a station to play it, the playing one to stop
//!   │ Radio Paradise                 › │
//!   └──────────────────────────────────┘
//!
//! The file, one station a line — `Name | URL`, or an .m3u's `#EXTINF:-1,Name` before the URL, or
//! a bare URL. MP3 and AAC streams (Icecast/SHOUTcast) and HLS (.m3u8); a .pls/.m3u link is followed
//! to its first stream.

use crate::canvas::{Canvas, H, W};
use crate::kit::{self, Row, Trail};
use crate::text::FontSet;
use crate::theme::Theme;

/// Where the stations come from, on the drive.
pub const FILE: &str = "/contents/cinder_radio.txt";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Station {
    pub name: String,
    pub url: String,
}

fn good_url(u: &str) -> bool {
    (u.starts_with("http://") || u.starts_with("https://"))
        && u.len() <= 512
        && u.chars().all(|c| c.is_ascii_graphic())
}

/// The stations in `text`, in file order, at most 200. Lines that are not a station are skipped.
pub fn parse_stations(text: &str) -> Vec<Station> {
    let mut out = Vec::new();
    let mut pending: Option<String> = None;
    for line in text.lines() {
        let l = line.trim().trim_start_matches('\u{feff}');
        if let Some(rest) = l.strip_prefix("#EXTINF:") {
            pending = rest.split_once(',').map(|(_, n)| n.trim().to_string());
            continue;
        }
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        let (name, url) = match l.rsplit_once('|') {
            Some((n, u)) => (n.trim().to_string(), u.trim()),
            None => (pending.take().unwrap_or_default(), l),
        };
        if !good_url(url) {
            continue;
        }
        let name = if name.is_empty() {
            url.split("://")
                .nth(1)
                .unwrap_or(url)
                .split('/')
                .next()
                .unwrap_or(url)
                .to_string()
        } else {
            name.chars().take(60).collect()
        };
        out.push(Station {
            name,
            url: url.to_string(),
        });
        if out.len() == 200 {
            break;
        }
    }
    out
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum State {
    #[default]
    Off,
    Connecting,
    Playing,
}

/// What the shell says: `state=off|connecting|playing`, `station=<index>`, `title=<now playing>`,
/// `note=<why it stopped>`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Status {
    pub state: State,
    pub station: Option<usize>,
    pub title: String,
    pub note: String,
}

pub fn parse_status(text: &str) -> Status {
    let mut s = Status::default();
    for line in text.lines() {
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let v = v.trim();
        match k.trim() {
            "state" => {
                s.state = match v {
                    "playing" => State::Playing,
                    "connecting" => State::Connecting,
                    _ => State::Off,
                }
            }
            "station" => s.station = v.parse().ok(),
            "title" => s.title = v.chars().filter(|c| !c.is_control()).take(80).collect(),
            "note" => s.note = v.chars().filter(|c| !c.is_control()).take(60).collect(),
            _ => {}
        }
    }
    s
}

pub fn strip_line(s: &Status, stations: &[Station]) -> String {
    let name = s
        .station
        .and_then(|i| stations.get(i))
        .map(|st| st.name.as_str())
        .unwrap_or("");
    match s.state {
        State::Off if !s.note.is_empty() => s.note.clone(),
        State::Off => crate::i18n::text("STOPPED").to_string(),
        State::Connecting => format!("{} · {name}", crate::i18n::text("CONNECTING")),
        State::Playing if s.title.is_empty() => {
            format!("{} · {name}", crate::i18n::text("PLAYING"))
        }
        State::Playing => format!("{} · {}", crate::i18n::text("PLAYING"), s.title),
    }
}

// ── layout, shared by render and hit ────────────────────────────────────────────────────────────

const FOOT_H: i32 = 44;

fn layout(stations: usize) -> (i32, i32, i32, usize) {
    let strip_y = crate::chrome::HEADER_BOTTOM;
    let section_y = strip_y + kit::STRIP_H;
    let list_y = section_y + kit::SECTION_H;
    let room = (H as i32 - FOOT_H - list_y).max(0) / kit::ROW_H();
    (strip_y, section_y, list_y, stations.min(room as usize))
}

/// FIND, on the STATIONS label: search radio-browser.info for more.
pub fn hit_find(x: i32, y: i32) -> bool {
    let (_, section_y, _, _) = layout(0);
    kit::section_action_hit(section_y, x, y)
}

/// The station under a tap, if any.
pub fn hit(x: i32, y: i32, stations: usize) -> Option<usize> {
    let (_, _, list_y, rows) = layout(stations);
    if y < list_y || x < 0 || x >= W as i32 {
        return None;
    }
    let i = ((y - list_y) / kit::ROW_H()) as usize;
    (i < rows).then_some(i)
}

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, stations: &[Station], s: &Status) {
    c.fill(t.bg);
    crate::chrome::header(c, t, f, crate::i18n::text("Internet radio"), None);
    let (strip_y, section_y, list_y, rows) = layout(stations.len());
    kit::strip(c, t, f, strip_y, &strip_line(s, stations));
    kit::section_label(c, t, f, section_y, "STATIONS", Some("FIND"));
    let mut y = list_y;
    for (i, st) in stations.iter().take(rows).enumerate() {
        let here = s.station == Some(i);
        let trail = match s.state {
            State::Playing if here => Trail::Tag(crate::i18n::text("PLAYING")),
            State::Connecting if here => Trail::Value("…"),
            _ => Trail::Open(""),
        };
        let host = st
            .url
            .split("://")
            .nth(1)
            .unwrap_or("")
            .split('/')
            .next()
            .unwrap_or("");
        y = kit::row(
            c,
            t,
            f,
            y,
            kit::ROW_H(),
            &Row::new(&st.name).sub(host).trail(trail),
        );
    }
    if stations.is_empty() {
        kit::row(
            c,
            t,
            f,
            y,
            kit::ROW_H(),
            &Row::new(crate::i18n::text("No stations yet")).sub(crate::i18n::text(
                "Add them to cinder_radio.txt on the drive",
            )),
        );
    }
}

// ── Internet radio ▸ FIND: stations from radio-browser.info ────────────────────────────────────────

/// One search result: the name, its stream, and the line under it ("AAC 128 · GB · jazz").
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Found {
    pub name: String,
    pub url: String,
    pub sub: String,
}

/// The result under a tap, if any.
pub fn hit_found(x: i32, y: i32, found: usize) -> Option<usize> {
    hit(x, y, found)
}

pub struct FoundView<'a> {
    pub query: &'a str,
    pub found: &'a [Found],
    pub searching: bool,
    /// Why there is nothing (no network, the search failed), when there is nothing.
    pub note: &'a str,
    /// The stations already in the list, to mark the results that are in it.
    pub have: &'a [Station],
}

pub fn render_found(c: &mut Canvas, t: &Theme, f: &FontSet, v: &FoundView) {
    c.fill(t.bg);
    crate::chrome::header(c, t, f, crate::i18n::text("Find a station"), None);
    let (strip_y, section_y, list_y, rows) = layout(v.found.len());
    let strip = if v.searching {
        crate::ui_format!("SEARCHING · {}", v.query)
    } else if !v.note.is_empty() {
        v.note.to_string()
    } else {
        crate::ui_format!("{} FOUND · {}", v.found.len(), v.query)
    };
    kit::strip(c, t, f, strip_y, &strip);
    kit::section_label(c, t, f, section_y, "TAP TO ADD AND PLAY", None);
    let mut y = list_y;
    for st in v.found.iter().take(rows) {
        let added = v.have.iter().any(|h| h.url == st.url);
        let trail = if added {
            Trail::Tag(crate::i18n::text("ADDED"))
        } else {
            Trail::Open("")
        };
        y = kit::row(
            c,
            t,
            f,
            y,
            kit::ROW_H(),
            &Row::new(&st.name).sub(&st.sub).trail(trail),
        );
    }
    if v.found.is_empty() && !v.searching {
        kit::row(
            c,
            t,
            f,
            y,
            kit::ROW_H(),
            &Row::new(crate::i18n::text("No stations found"))
                .sub(crate::i18n::text("Try another name, or a genre")),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stations_from_every_shape_of_line() {
        let s = parse_stations(
            "\u{feff}# my stations\n\
             Groove Salad | https://ice1.somafm.com/groovesalad-128-mp3\n\
             #EXTM3U\n#EXTINF:-1,Radio Paradise\nhttp://stream.radioparadise.com/mp3-192\n\
             http://example.org:8000/live\n\
             Bad | ftp://example.org/x\n\
             Spaces | http://example.org/a b\n\
             \n",
        );
        let names: Vec<&str> = s.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(
            names,
            ["Groove Salad", "Radio Paradise", "example.org:8000"]
        );
        assert_eq!(s[0].url, "https://ice1.somafm.com/groovesalad-128-mp3");
    }

    #[test]
    fn status_and_strip() {
        let st = parse_stations("A | http://a/1\nB | http://b/2\n");
        let s = parse_status("state=playing\nstation=1\ntitle=Artist - Song\n");
        assert_eq!((s.state, s.station), (State::Playing, Some(1)));
        assert_eq!(strip_line(&s, &st), "PLAYING · Artist - Song");
        let s = parse_status("state=connecting\nstation=0\n");
        assert_eq!(strip_line(&s, &st), "CONNECTING · A");
        let s = parse_status("state=off\nnote=This station is encrypted\n");
        assert_eq!(strip_line(&s, &st), "This station is encrypted");
        assert_eq!(parse_status("").state, State::Off);
    }

    #[test]
    fn find_is_on_the_label() {
        let (_, section_y, _, _) = layout(0);
        assert!(hit_find(W as i32 - 30, section_y + 10));
        assert!(
            !hit_find(30, section_y + 10),
            "the left half is the label, not the action"
        );
        assert!(!hit_find(W as i32 - 30, section_y - 10));
    }

    #[test]
    fn taps_land_on_stations_only() {
        let (_, _, list_y, _) = layout(2);
        assert_eq!(hit(200, list_y + 5, 2), Some(0));
        assert_eq!(hit(200, list_y + kit::ROW_H() + 5, 2), Some(1));
        assert_eq!(hit(200, list_y + 2 * kit::ROW_H() + 5, 2), None);
        assert_eq!(hit(200, list_y - 5, 2), None);
    }
}
