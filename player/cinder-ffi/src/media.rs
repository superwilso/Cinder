//! Settings ▸ Wi-Fi ▸ Media servers: browse a music server on the home network (UPnP/DLNA
//! ContentDirectory: MiniDLNA on a NAS, Jellyfin, Plex, Emby, Serviio, Kodi, MusicBee's and
//! Windows Media Player's sharing) and play from it.
//!
//! Search is one SSDP search for media servers, as the Speakers page does for speakers
//! (`cast.rs`, whose HTTP, SOAP and XML helpers this uses). Browsing is the `Browse` action, a
//! folder at a time. A song is played by the internet radio's player (curl, then the MP3, AAC,
//! FLAC or WAV decoder, to the jack or Bluetooth) as radio station -3, and the next song in the
//! folder is sent when one ends, until the folder does.
//!
//! What a server offers that those decoders cannot play (ALAC and AAC in MP4, Ogg, DSD) is listed
//! but not played: a server's other `<res>` for the same song (many transcode to MP3 or WAV) is
//! taken when there is one.

use std::collections::HashSet;
use std::net::UdpSocket;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::cast::{attr, elements, elements_full, http, join, location, soap, tag, unescape};

#[derive(Clone, Debug, PartialEq)]
pub struct Server {
    pub name: String,
    /// The ContentDirectory control URL.
    pub cd: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Song {
    pub title: String,
    pub artist: String,
    pub album: String,
    /// Empty when no `<res>` is in a format the player decodes.
    pub url: String,
    pub dur_ms: i64,
    /// The cover (`upnp:albumArtURI`), "" for none.
    pub art: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Entry {
    Folder {
        id: String,
        title: String,
        count: Option<u32>,
    },
    Song(Song),
}

/// The most entries one folder lists (servers page through; a folder of 10,000 songs is a
/// library, not a folder).
const MAX_ENTRIES: usize = 2000;

// ── the server's description ──────────────────────────────────────────────────────────────────────

/// A media server's description (from its SSDP LOCATION). None without a ContentDirectory.
pub fn parse_server(xml: &str, location: &str) -> Option<Server> {
    let base = tag(xml, "URLBase").unwrap_or_else(|| location.to_string());
    let cd = elements(xml, "service").into_iter().find_map(|s| {
        tag(s, "serviceType")?
            .contains(":service:ContentDirectory:")
            .then(|| tag(s, "controlURL"))
            .flatten()
            .map(|u| join(&base, &u))
    })?;
    let name = tag(xml, "friendlyName").unwrap_or_else(|| "Media server".into());
    Some(Server {
        name: name.chars().take(60).collect(),
        cd,
    })
}

const SEARCH: &str =
    "M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nMAN: \"ssdp:discover\"\r\nMX: 2\r\n\
                      ST: urn:schemas-upnp-org:device:MediaServer:1\r\n\r\n";

/// Every media server that answers within `wait`, by name.
pub fn discover(wait: Duration) -> Result<Vec<Server>, String> {
    let sock = UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
    let _ = sock.set_multicast_ttl_v4(2);
    let _ = sock.set_read_timeout(Some(Duration::from_millis(200)));
    let to = ("239.255.255.250", 1900);
    sock.send_to(SEARCH.as_bytes(), to)
        .map_err(|e| e.to_string())?;
    let t0 = Instant::now();
    let mut resent = false;
    let mut seen = HashSet::new();
    let mut buf = [0u8; 2048];
    while t0.elapsed() < wait {
        if !resent && t0.elapsed() > Duration::from_millis(800) {
            let _ = sock.send_to(SEARCH.as_bytes(), to);
            resent = true;
        }
        if let Ok((n, _)) = sock.recv_from(&mut buf) {
            if let Some(l) = location(&String::from_utf8_lossy(&buf[..n])) {
                seen.insert(l);
            }
        }
    }
    // ponytail: descriptions fetched one after another; in parallel (as cast.rs does) if a home
    // with many servers waits.
    let mut v: Vec<Server> = seen
        .into_iter()
        .take(16)
        .filter_map(|loc| match http("GET", &loc, &[], "") {
            Ok((200, xml)) => parse_server(&xml, &loc),
            _ => None,
        })
        .collect();
    v.sort_by_key(|s| s.name.to_lowercase());
    v.dedup_by(|a, b| a.cd == b.cd);
    Ok(v)
}

// ── browsing ──────────────────────────────────────────────────────────────────────────────────────

/// Can the radio's decoders play this `<res>` type? (`audio/mpeg`, from protocolInfo's third field.)
pub fn playable(mime: &str) -> bool {
    let m = mime.to_ascii_lowercase();
    matches!(
        m.split(';').next().unwrap_or("").trim(),
        "audio/mpeg"
            | "audio/mp3"
            | "audio/x-mpeg"
            | "audio/flac"
            | "audio/x-flac"
            | "audio/wav"
            | "audio/x-wav"
            | "audio/wave"
            | "audio/aac"
            | "audio/aacp"
            | "audio/x-aac"
            | "audio/vnd.dlna.adts"
    )
}

/// A DIDL-Lite document's folders and songs, in the server's order. Videos and pictures are left
/// out; a song with no playable `<res>` keeps an empty `url`.
pub fn parse_didl(didl: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    // Containers and items, in document order: find each opening tag's position.
    let mut at = Vec::new();
    for name in ["container", "item"] {
        let mut from = 0;
        for (open, body) in elements_full(didl, name) {
            let pos = didl[from..].find(open).map(|p| p + from).unwrap_or(from);
            from = pos + 1;
            at.push((pos, name, open, body));
        }
    }
    at.sort_by_key(|e| e.0);
    for (_, name, open, body) in at {
        let title = tag(body, "title").unwrap_or_default();
        if name == "container" {
            let Some(id) = attr(open, "id") else { continue };
            out.push(Entry::Folder {
                id,
                title,
                count: attr(open, "childCount").and_then(|c| c.parse().ok()),
            });
            continue;
        }
        let class = tag(body, "class").unwrap_or_default();
        if !class.starts_with("object.item.audioItem") {
            continue;
        }
        let mut song = Song {
            title,
            artist: tag(body, "artist")
                .or_else(|| tag(body, "creator"))
                .unwrap_or_default(),
            album: tag(body, "album").unwrap_or_default(),
            art: tag(body, "albumArtURI")
                .filter(|u| u.starts_with("http://"))
                .unwrap_or_default(),
            ..Default::default()
        };
        for (ropen, rbody) in elements_full(body, "res") {
            let mime = attr(ropen, "protocolInfo")
                .and_then(|p| p.split(':').nth(2).map(str::to_string))
                .unwrap_or_default();
            let url = unescape(rbody.trim());
            if song.dur_ms == 0 {
                song.dur_ms = attr(ropen, "duration")
                    .and_then(|d| crate::cast::hms_ms(&d))
                    .unwrap_or(0);
            }
            if song.url.is_empty()
                && playable(&mime)
                && (url.starts_with("http://") || url.starts_with("https://"))
            {
                song.url = url;
            }
        }
        out.push(Entry::Song(song));
    }
    out
}

/// One folder of `server` (`"0"` is the top), every page of it up to [`MAX_ENTRIES`].
pub fn browse(server: &Server, id: &str) -> Result<Vec<Entry>, String> {
    pages(
        server,
        "Browse",
        &[("ObjectID", id), ("BrowseFlag", "BrowseDirectChildren")],
    )
}

/// The songs on `server` whose title, artist or album holds `text` (ContentDirectory `Search`;
/// not every server has it, and the error says so).
pub fn search(server: &Server, text: &str) -> Result<Vec<Entry>, String> {
    pages(
        server,
        "Search",
        &[("ContainerID", "0"), ("SearchCriteria", &criteria(text))],
    )
}

/// The SearchCriteria for [`search`]: a quote or backslash in the text is escaped, as UPnP's
/// grammar asks.
pub fn criteria(text: &str) -> String {
    let q = text.replace('\\', "\\\\").replace('"', "\\\"");
    format!(
        "upnp:class derivedfrom \"object.item.audioItem\" and (dc:title contains \"{q}\" or \
         upnp:artist contains \"{q}\" or upnp:album contains \"{q}\")"
    )
}

/// An action that answers in pages of DIDL-Lite, asked until it is all in (or [`MAX_ENTRIES`]).
fn pages(server: &Server, action: &str, first: &[(&str, &str)]) -> Result<Vec<Entry>, String> {
    let mut out = Vec::new();
    loop {
        let start = out.len().to_string();
        let mut args = first.to_vec();
        args.extend([
            ("Filter", "*"),
            ("StartingIndex", start.as_str()),
            ("RequestedCount", "200"),
            ("SortCriteria", ""),
        ]);
        let resp = soap(&server.cd, "ContentDirectory", action, &args)?;
        let page = parse_didl(&tag(&resp, "Result").unwrap_or_default());
        let total: usize = tag(&resp, "TotalMatches")
            .and_then(|t| t.parse().ok())
            .unwrap_or(0);
        let returned: usize = tag(&resp, "NumberReturned")
            .and_then(|t| t.parse().ok())
            .unwrap_or(page.len());
        out.extend(page);
        if returned == 0 || out.len() >= total || out.len() >= MAX_ENTRIES {
            break;
        }
    }
    out.truncate(MAX_ENTRIES);
    Ok(out)
}

// ── playing ───────────────────────────────────────────────────────────────────────────────────────

/// The radio station number the shell plays these songs as.
pub const STATION: &str = "-3";

struct Queue {
    songs: Vec<Song>,
    at: usize,
    /// The shell has said it is connecting or playing the song at `at`.
    started: bool,
    /// When `at` was asked for, so an "off" from the song before is not taken as this one's end.
    asked: Instant,
    /// Held by the pause key.
    paused: bool,
    /// How far into the song, at `since` (None while not playing).
    pos_ms: i64,
    since: Option<Instant>,
}

/// What the shell is asked to do next ([`take`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ask {
    Play(String),
    Pause,
    /// Go on from the pause; the URL is for when the stream died meanwhile.
    Resume(String),
}

static QUEUE: Mutex<Option<Queue>> = Mutex::new(None);
static ASK: Mutex<Option<Ask>> = Mutex::new(None);

/// The first playable song at or after `from`.
fn next_playable(songs: &[Song], from: usize) -> Option<usize> {
    (from..songs.len()).find(|&i| !songs[i].url.is_empty())
}

/// Play `songs` from `start` (or the first playable one after it), then the rest in order.
/// False when none of them can be played.
pub fn play(songs: Vec<Song>, start: usize) -> bool {
    let Some(at) = next_playable(&songs, start) else {
        return false;
    };
    *ASK.lock().unwrap() = Some(Ask::Play(songs[at].url.clone()));
    *QUEUE.lock().unwrap() = Some(Queue {
        songs,
        at,
        started: false,
        asked: Instant::now(),
        paused: false,
        pos_ms: 0,
        since: None,
    });
    true
}

/// For the shell, once a second: what to do with station -3.
pub fn take() -> Option<Ask> {
    ASK.lock().unwrap().take()
}

/// A server's songs are playing (or paused).
pub fn active() -> bool {
    QUEUE.lock().unwrap().is_some()
}

/// The pause key: hold the stream, or go on with it.
pub fn toggle() {
    let mut g = QUEUE.lock().unwrap();
    let Some(q) = g.as_mut() else { return };
    if q.paused {
        q.paused = false;
        q.since = Some(Instant::now());
        *ASK.lock().unwrap() = Some(Ask::Resume(q.songs[q.at].url.clone()));
    } else {
        q.paused = true;
        if let Some(t) = q.since.take() {
            q.pos_ms += t.elapsed().as_millis() as i64;
        }
        *ASK.lock().unwrap() = Some(Ask::Pause);
    }
}

/// The next (`1`) or previous (`-1`) playable song in the folder. Previous more than 3 s into a
/// song starts it again, as the player's own key does. False at either end.
pub fn skip(delta: i32) -> bool {
    let mut g = QUEUE.lock().unwrap();
    let Some(q) = g.as_mut() else { return false };
    let into = q.pos_ms + q.since.map_or(0, |t| t.elapsed().as_millis() as i64);
    let to = if delta > 0 {
        next_playable(&q.songs, q.at + 1)
    } else if into > 3000 {
        Some(q.at)
    } else {
        (0..q.at).rev().find(|&i| !q.songs[i].url.is_empty())
    };
    let Some(i) = to else { return false };
    q.at = i;
    q.started = false;
    q.paused = false;
    q.pos_ms = 0;
    q.since = None;
    q.asked = Instant::now();
    *ASK.lock().unwrap() = Some(Ask::Play(q.songs[i].url.clone()));
    true
}

/// How far into the song playing, and whether it is paused.
pub fn position() -> Option<(i64, bool)> {
    let g = QUEUE.lock().unwrap();
    let q = g.as_ref()?;
    Some((
        q.pos_ms + q.since.map_or(0, |t| t.elapsed().as_millis() as i64),
        q.paused,
    ))
}

/// Forget the queue (the shell stops the stream itself).
pub fn stop() {
    *QUEUE.lock().unwrap() = None;
    *ASK.lock().unwrap() = None;
}

/// The song playing, its place and the count, while a queue is on.
pub fn now() -> Option<(Song, usize, usize)> {
    let q = QUEUE.lock().unwrap();
    q.as_ref()
        .map(|q| (q.songs[q.at].clone(), q.at + 1, q.songs.len()))
}

/// The radio's status (`state=…\nstation=…\nnote=…`), as the shell reports it. Moves to the next
/// song when one ends; ends the queue when the last one has, when a song fails (the note, for the
/// screen), or when another station takes the output.
pub fn on_status(text: &str) -> Option<String> {
    let field = |k: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(k).and_then(|v| v.strip_prefix('=')))
            .unwrap_or("")
            .trim()
            .to_string()
    };
    let (st, station, note) = (field("state"), field("station"), field("note"));
    let mut guard = QUEUE.lock().unwrap();
    let q = guard.as_mut()?;
    let ours = station == STATION;
    match st.as_str() {
        "connecting" if ours => q.started = true,
        "playing" if ours => {
            q.started = true;
            if q.since.is_none() && !q.paused {
                q.since = Some(Instant::now());
            }
        }
        "connecting" | "playing" => *guard = None,
        "off" if q.started || q.asked.elapsed() > Duration::from_secs(30) => {
            if !note.is_empty() && ours {
                *guard = None;
                return Some(note);
            }
            match next_playable(&q.songs, q.at + 1) {
                Some(i) => {
                    q.at = i;
                    q.started = false;
                    q.pos_ms = 0;
                    q.since = None;
                    q.asked = Instant::now();
                    *ASK.lock().unwrap() = Some(Ask::Play(q.songs[i].url.clone()));
                }
                None => *guard = None,
            }
        }
        _ => {}
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// MiniDLNA 1.3's answer to Browse of a music folder (one folder, two songs, a video), cut down.
    const BROWSE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/"><s:Body><u:BrowseResponse xmlns:u="urn:schemas-upnp-org:service:ContentDirectory:1"><Result>&lt;DIDL-Lite xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:upnp="urn:schemas-upnp-org:metadata-1-0/upnp/" xmlns="urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/"&gt;&lt;container id="1$4" parentID="1" restricted="1" searchable="1" childCount="12"&gt;&lt;dc:title&gt;Albums&lt;/dc:title&gt;&lt;upnp:class&gt;object.container.storageFolder&lt;/upnp:class&gt;&lt;/container&gt;&lt;item id="64$1" parentID="64" restricted="1"&gt;&lt;dc:title&gt;Hoppípolla&lt;/dc:title&gt;&lt;upnp:class&gt;object.item.audioItem.musicTrack&lt;/upnp:class&gt;&lt;dc:creator&gt;Sigur Rós&lt;/dc:creator&gt;&lt;upnp:artist&gt;Sigur Rós&lt;/upnp:artist&gt;&lt;upnp:album&gt;Takk...&lt;/upnp:album&gt;&lt;res size="35102213" duration="0:04:28.000" protocolInfo="http-get:*:audio/x-flac:*"&gt;http://192.168.1.20:8200/MediaItems/22.flac&lt;/res&gt;&lt;/item&gt;&lt;item id="64$2" parentID="64" restricted="1"&gt;&lt;dc:title&gt;Glósóli&lt;/dc:title&gt;&lt;upnp:class&gt;object.item.audioItem.musicTrack&lt;/upnp:class&gt;&lt;dc:creator&gt;Sigur Rós&lt;/dc:creator&gt;&lt;res duration="0:06:15.000" protocolInfo="http-get:*:audio/mp4:*"&gt;http://192.168.1.20:8200/MediaItems/23.m4a&lt;/res&gt;&lt;res protocolInfo="http-get:*:audio/mpeg:DLNA.ORG_PN=MP3"&gt;http://192.168.1.20:8200/Transcode/23.mp3?a=1&amp;amp;b=2&lt;/res&gt;&lt;/item&gt;&lt;item id="99" parentID="64"&gt;&lt;dc:title&gt;clip&lt;/dc:title&gt;&lt;upnp:class&gt;object.item.videoItem&lt;/upnp:class&gt;&lt;/item&gt;&lt;/DIDL-Lite&gt;</Result><NumberReturned>4</NumberReturned><TotalMatches>4</TotalMatches><UpdateID>7</UpdateID></u:BrowseResponse></s:Body></s:Envelope>"#;

    /// Bytes from the network reach these parsers inside the Home app, where a panic ends it.
    /// Seeded mutation of real answers; `CINDER_FUZZ_N=200000 cargo test --release` for a long run.
    #[test]
    fn the_parsers_survive_mangled_answers() {
        let n: usize = std::env::var("CINDER_FUZZ_N")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(3000);
        let didl = tag(BROWSE, "Result").unwrap();
        let seeds = [
            didl.as_str(),
            BROWSE,
            "<item><res duration=\"9:99:99.9\">x</res></item>",
        ];
        let bits = [
            "<",
            ">",
            "\"",
            "&",
            "&amp;",
            "&lt;",
            "=",
            ":",
            ".",
            "0",
            "é",
            "€",
            "𝄞",
            " ",
            "/",
            "<item>",
            "</item>",
            "<container id=\"",
            "<res ",
            "protocolInfo=\"",
            "duration=\"",
            "object.item.audioItem",
            "http://",
            "<dc:title>",
            "</res>",
            "\u{0}",
        ];
        let mut r: u64 = 0x9e37_79b9_7f4a_7c15;
        let mut rnd = |m: usize| {
            r ^= r << 13;
            r ^= r >> 7;
            r ^= r << 17;
            (r % m.max(1) as u64) as usize
        };
        for _ in 0..n {
            let mut v: Vec<char> = seeds[rnd(seeds.len())].chars().collect();
            for _ in 0..1 + rnd(8) {
                let at = rnd(v.len() + 1);
                match rnd(3) {
                    0 if at < v.len() => {
                        let end = (at + 1 + rnd(40)).min(v.len());
                        v.drain(at..end);
                    }
                    _ => {
                        let ins: Vec<char> = bits[rnd(bits.len())].chars().collect();
                        v.splice(at..at, ins);
                    }
                }
            }
            let s: String = v.into_iter().collect();
            let _ = parse_didl(&s);
            let _ = parse_server(&s, "http://192.168.1.20:8200/rootDesc.xml");
            let _ = on_status(&s);
            let _ = criteria(&s);
            let _ = crate::cast::hms_ms(&s);
        }
    }

    #[test]
    fn reads_folders_and_songs_in_order_and_picks_a_playable_res() {
        let v = parse_didl(&tag(BROWSE, "Result").unwrap());
        assert_eq!(v.len(), 3, "the video is left out: {v:?}");
        assert_eq!(
            v[0],
            Entry::Folder {
                id: "1$4".into(),
                title: "Albums".into(),
                count: Some(12)
            }
        );
        let Entry::Song(a) = &v[1] else { panic!() };
        assert_eq!(
            (a.title.as_str(), a.artist.as_str(), a.album.as_str()),
            ("Hoppípolla", "Sigur Rós", "Takk...")
        );
        assert_eq!(a.url, "http://192.168.1.20:8200/MediaItems/22.flac");
        assert_eq!(a.dur_ms, 268_000);
        let Entry::Song(b) = &v[2] else { panic!() };
        assert_eq!(
            b.url, "http://192.168.1.20:8200/Transcode/23.mp3?a=1&b=2",
            "the MP4 is skipped for the server's MP3"
        );
        assert_eq!(
            b.artist, "Sigur Rós",
            "dc:creator when there is no upnp:artist"
        );
    }

    #[test]
    fn a_server_description_gives_its_content_directory() {
        let xml = r#"<root><URLBase>http://192.168.1.20:8200/</URLBase><device><friendlyName>NAS: music</friendlyName>
            <serviceList><service><serviceType>urn:schemas-upnp-org:service:ConnectionManager:1</serviceType><controlURL>/ctl/ConnectionMgr</controlURL></service>
            <service><serviceType>urn:schemas-upnp-org:service:ContentDirectory:1</serviceType><controlURL>/ctl/ContentDir</controlURL></service></serviceList></device></root>"#;
        assert_eq!(
            parse_server(xml, "http://192.168.1.20:8200/rootDesc.xml"),
            Some(Server {
                name: "NAS: music".into(),
                cd: "http://192.168.1.20:8200/ctl/ContentDir".into()
            })
        );
        assert_eq!(
            parse_server("<root><friendlyName>TV</friendlyName></root>", "http://x/"),
            None
        );
    }

    #[test]
    fn search_criteria_escape_the_text() {
        assert_eq!(
            criteria(r#"say "hi""#),
            r#"upnp:class derivedfrom "object.item.audioItem" and (dc:title contains "say \"hi\"" or upnp:artist contains "say \"hi\"" or upnp:album contains "say \"hi\"")"#
        );
    }

    #[test]
    fn playable_types() {
        for m in [
            "audio/mpeg",
            "audio/x-flac",
            "audio/wav",
            "audio/vnd.dlna.adts",
            "AUDIO/FLAC",
        ] {
            assert!(playable(m), "{m}");
        }
        for m in [
            "audio/mp4",
            "audio/x-m4a",
            "audio/ogg",
            "audio/x-dsf",
            "video/mp4",
            "",
        ] {
            assert!(!playable(m), "{m}");
        }
    }

    /// A real server, browsed over HTTP the way the player does (a little server on localhost).
    #[test]
    fn browse_asks_the_server_page_by_page() {
        use std::io::{Read, Write};
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let t = std::thread::spawn(move || {
            let mut starts = Vec::new();
            for page in 0..2 {
                let (mut c, _) = l.accept().unwrap();
                let mut buf = vec![0u8; 8192];
                let n = c.read(&mut buf).unwrap();
                let req = String::from_utf8_lossy(&buf[..n]).into_owned();
                starts.push(tag(&req, "StartingIndex").unwrap_or_default());
                let item = |i: u32| {
                    format!("&lt;item id=\"{i}\"&gt;&lt;dc:title&gt;S{i}&lt;/dc:title&gt;&lt;upnp:class&gt;object.item.audioItem.musicTrack&lt;/upnp:class&gt;&lt;res protocolInfo=\"http-get:*:audio/mpeg:*\"&gt;http://h/{i}.mp3&lt;/res&gt;&lt;/item&gt;")
                };
                let body = format!(
                    "<s:Envelope><s:Body><u:BrowseResponse><Result>&lt;DIDL-Lite&gt;{}{}&lt;/DIDL-Lite&gt;</Result><NumberReturned>2</NumberReturned><TotalMatches>4</TotalMatches></u:BrowseResponse></s:Body></s:Envelope>",
                    item(page * 2),
                    item(page * 2 + 1)
                );
                let _ = c.write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
                        body.len()
                    )
                    .as_bytes(),
                );
            }
            starts
        });
        let server = Server {
            name: "t".into(),
            cd: format!("http://127.0.0.1:{port}/ctl"),
        };
        let v = browse(&server, "64").unwrap();
        assert_eq!(v.len(), 4);
        assert_eq!(t.join().unwrap(), ["0", "2"]);
    }

    fn song(t: &str, url: &str) -> Song {
        Song {
            title: t.into(),
            url: url.into(),
            ..Default::default()
        }
    }

    /// One test for the queue: it is process-wide, so its steps must not run beside each other.
    #[test]
    fn the_queue_plays_on_skips_what_it_cannot_and_stops_when_told() {
        let songs = vec![
            song("a", "http://h/a"),
            song("m4a", ""),
            song("c", "http://h/c"),
        ];
        assert!(play(songs.clone(), 0));
        assert_eq!(take(), Some(Ask::Play("http://h/a".into())));
        assert_eq!(take(), None);
        on_status("state=connecting\nstation=-3\n");
        on_status("state=playing\nstation=-3\n");
        assert_eq!(now().unwrap().1, 1);
        // The end of song a: the one with no playable file is passed over.
        on_status("state=off\nstation=-3\n");
        assert_eq!(take(), Some(Ask::Play("http://h/c".into())));
        assert_eq!(
            now().map(|n| (n.0.title, n.1, n.2)),
            Some(("c".into(), 3, 3))
        );
        // An "off" before c has started is a.'s, not c's.
        on_status("state=off\nstation=-3\n");
        assert_eq!(take(), None);
        on_status("state=playing\nstation=-3\n");
        on_status("state=off\nstation=-3\n");
        assert!(now().is_none(), "after the last song the queue is done");

        // A failure ends it and says why; another station ends it quietly.
        assert!(play(songs.clone(), 0));
        on_status("state=connecting\nstation=-3\n");
        assert_eq!(
            on_status("state=off\nstation=-3\nnote=The station did not answer\n").as_deref(),
            Some("The station did not answer")
        );
        assert!(now().is_none());
        assert!(
            play(songs.clone(), 1),
            "from a song it cannot play: the next it can"
        );
        assert_eq!(take(), Some(Ask::Play("http://h/c".into())));
        on_status("state=connecting\nstation=4\n");
        assert!(now().is_none());
        assert!(!play(vec![song("m4a", "")], 0));

        // The keys: pause and go on; next and previous; previous at the start stays put.
        assert!(play(songs.clone(), 0));
        take();
        on_status("state=playing\nstation=-3\n");
        toggle();
        assert_eq!(take(), Some(Ask::Pause));
        assert!(position().unwrap().1, "paused");
        toggle();
        assert_eq!(take(), Some(Ask::Resume("http://h/a".into())));
        assert!(!skip(-1), "nothing before the first");
        assert!(skip(1));
        assert_eq!(take(), Some(Ask::Play("http://h/c".into())));
        assert!(!skip(1), "nothing after the last");
        assert!(skip(-1), "back over the one it cannot play");
        assert_eq!(take(), Some(Ask::Play("http://h/a".into())));
        stop();
        assert!(!active());
    }
}
