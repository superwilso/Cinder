//! What is making the sound when it is not the library (`cinder_set_live_source`, cinder.h's
//! `CINDER_LIVE_*`; 0 = the library), and the two lines Now Playing shows for it.
//!
//! Moved out of nav.rs (2026-11-23) as a pure function of what each source's page knows, so the
//! rules can be tested without an `App`. `App::live_lines` gathers the inputs.

pub const RADIO: u8 = 1;
pub const DLNA: u8 = 2;
pub const AIRPLAY: u8 = 3;
pub const FM: u8 = 4;
pub const RECEIVER: u8 = 5;
pub const USB_DAC: u8 = 6;

/// What an AirPlay sender says is playing (shairport-sync's metadata pipe: `minm`, `asar`, `asal`).
#[derive(Clone, Default, Debug, PartialEq)]
pub struct AirPlayMeta {
    pub title: String,
    pub artist: String,
    pub album: String,
}

/// Everything the lines are built from.
pub struct Inputs<'a> {
    /// The playing station's name, when the radio page knows it.
    pub station: Option<&'a str>,
    /// The stream's ICY title, "Artist - Song".
    pub radio_title: &'a str,
    /// A DLNA controller's own title for its link.
    pub dlna_title: &'a str,
    pub airplay: &'a AirPlayMeta,
    pub fm_khz: i32,
    pub rx_peer: &'a str,
    pub rx_codec: u32,
    pub usb_dac_fmt: Option<(u32, u32, u32)>,
}

/// Now Playing's title and second line for live source `kind`.
pub fn lines(kind: u8, i: &Inputs) -> (String, String) {
    let text = crate::i18n::text;
    // An ICY title is "Artist - Song".
    let split = |t: &str| match t.split_once(" - ") {
        Some((a, s)) if !s.trim().is_empty() => (s.trim().to_string(), a.trim().to_string()),
        _ => (t.trim().to_string(), String::new()),
    };
    match kind {
        RADIO => {
            let station = i
                .station
                .map(str::to_string)
                .unwrap_or_else(|| text("Internet radio").to_string());
            match split(i.radio_title) {
                (s, _) if s.is_empty() => (station, text("Internet radio").to_string()),
                (s, a) if a.is_empty() => (s, station),
                (s, a) => (s, format!("{a} · {station}")),
            }
        }
        DLNA => {
            let (s, a) = split(i.dlna_title);
            let from = text("Play to this player").to_string();
            match (s.is_empty(), a.is_empty()) {
                (true, _) => (from, String::new()),
                (false, true) => (s, from),
                (false, false) => (s, a),
            }
        }
        // The sender's song when it names one; "AirPlay" until then, and under a song with no
        // artist, so the source stays visible.
        AIRPLAY => {
            let m = i.airplay;
            let airplay = text("AirPlay").to_string();
            match (m.title.trim(), m.artist.trim()) {
                ("", _) => (airplay, String::new()),
                (t, "") => (t.to_string(), airplay),
                (t, a) => (t.to_string(), a.to_string()),
            }
        }
        FM => (
            format!("{:.1} MHz", i.fm_khz as f32 / 1000.0),
            text("FM radio").to_string(),
        ),
        RECEIVER => (
            if i.rx_peer.is_empty() {
                text("Bluetooth").to_string()
            } else {
                i.rx_peer.to_string()
            },
            if i.rx_codec == 0 {
                String::new()
            } else {
                crate::receiver::codec_label(i.rx_codec)
            },
        ),
        USB_DAC => (
            text("USB-DAC").to_string(),
            i.usb_dac_fmt
                .map(|(rate, bits, _)| format!("{:.1} kHz · {bits}-bit", rate as f32 / 1000.0))
                .unwrap_or_default(),
        ),
        _ => (String::new(), String::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs(airplay: &AirPlayMeta) -> Inputs<'_> {
        Inputs {
            station: None,
            radio_title: "",
            dlna_title: "",
            airplay,
            fm_khz: 98500,
            rx_peer: "",
            rx_codec: 0,
            usb_dac_fmt: None,
        }
    }

    #[test]
    fn airplay_shows_the_senders_song_and_the_source_until_it_names_one() {
        let none = AirPlayMeta::default();
        assert_eq!(lines(AIRPLAY, &inputs(&none)), ("AirPlay".into(), String::new()));
        let song = AirPlayMeta {
            title: "Holocene".into(),
            artist: "Bon Iver".into(),
            album: "Bon Iver, Bon Iver".into(),
        };
        assert_eq!(
            lines(AIRPLAY, &inputs(&song)),
            ("Holocene".into(), "Bon Iver".into())
        );
        let no_artist = AirPlayMeta {
            title: "Voice memo".into(),
            ..Default::default()
        };
        assert_eq!(
            lines(AIRPLAY, &inputs(&no_artist)),
            ("Voice memo".into(), "AirPlay".into())
        );
    }

    #[test]
    fn fm_reads_its_frequency() {
        let none = AirPlayMeta::default();
        assert_eq!(
            lines(FM, &inputs(&none)),
            ("98.5 MHz".into(), "FM radio".into())
        );
    }
}
