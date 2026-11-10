//! Opt-in switches: things someone asked for that ship OFF, so nothing changes for anyone who did
//! not switch them on. They live in ONE list so the rules cannot drift: off by default (an empty
//! [`Opts`] is all off), saved and loaded under a fixed key, cleared by Reset, and drawn as a
//! switch on their row — all from here.
//!
//! Adding one:
//! 1. A variant in [`Opt`], plus its line in `ALL` and `key` (the settings-file word; never rename
//!    a shipped one, or saved files lose it).
//! 2. Its row in `row`: a `settings::ROW_*` (the switch is then drawn and toggled for you), or
//!    `None` for a row on another screen that toggles it itself (Wi-Fi ▸ Options).
//! 3. If switching it must change something else at once, an arm in `App::set_opt`.
//! 4. Read it where it matters: `app.opt(Opt::X)`.
//!
//! A choice with more than two values (Swipe up, Now Playing button) is not a switch and stays its
//! own setting.

use crate::settings as s;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Opt {
    /// The pull-down panel (`quick.rs`).
    Quick,
    /// The Library's FOLDERS link (#45).
    FoldersTab,
    /// DELETE on Track information and the album page.
    DeleteMusic,
    /// An icon beside each Menu row (#53).
    MenuIcons,
    /// Now Playing's back arrow (#51).
    NpBack,
    /// Now Playing's "3 / 12" (#51).
    NpCount,
    /// Hold locks the player's buttons too (#51).
    HoldButtons,
    /// Wi-Fi ▸ Options: turn on at start, set the clock, send plays, check for updates, fetch
    /// lyrics, fetch art, play to this player. Wi-Fi ▸ AirPlay.
    WifiAutostart,
    WifiClock,
    WifiLb,
    WifiUpdates,
    WifiLyrics,
    WifiArt,
    WifiPlayTo,
    WifiAirplay,
    /// Wi-Fi ▸ Phone remote: kept on across restarts, serving whenever Wi-Fi is connected.
    WifiRemote,
}

impl Opt {
    pub const ALL: [Opt; 16] = [
        Opt::Quick,
        Opt::FoldersTab,
        Opt::DeleteMusic,
        Opt::MenuIcons,
        Opt::NpBack,
        Opt::NpCount,
        Opt::HoldButtons,
        Opt::WifiAutostart,
        Opt::WifiClock,
        Opt::WifiLb,
        Opt::WifiUpdates,
        Opt::WifiLyrics,
        Opt::WifiArt,
        Opt::WifiPlayTo,
        Opt::WifiAirplay,
        Opt::WifiRemote,
    ];

    /// The settings-file key.
    pub const fn key(self) -> &'static str {
        match self {
            Opt::Quick => "quick_settings",
            Opt::FoldersTab => "folders_tab",
            Opt::DeleteMusic => "delete_music",
            Opt::MenuIcons => "menu_icons",
            Opt::NpBack => "np_back",
            Opt::NpCount => "np_count",
            Opt::HoldButtons => "hold_buttons",
            Opt::WifiAutostart => "wifi_autostart",
            Opt::WifiClock => "wifi_clock",
            Opt::WifiLb => "wifi_lb",
            Opt::WifiUpdates => "wifi_updates",
            Opt::WifiLyrics => "wifi_lyrics",
            Opt::WifiArt => "wifi_art",
            Opt::WifiPlayTo => "wifi_play_to",
            Opt::WifiAirplay => "wifi_airplay",
            Opt::WifiRemote => "wifi_remote",
        }
    }

    pub fn from_key(k: &str) -> Option<Opt> {
        Self::ALL.into_iter().find(|o| o.key() == k)
    }

    /// The Settings row that is this switch, if it has one there.
    pub const fn row(self) -> Option<usize> {
        match self {
            Opt::Quick => Some(s::ROW_QUICK),
            Opt::FoldersTab => Some(s::ROW_FOLDERS_TAB),
            Opt::DeleteMusic => Some(s::ROW_DELETE),
            Opt::MenuIcons => Some(s::ROW_MENU_ICONS),
            Opt::NpBack => Some(s::ROW_NP_BACK),
            Opt::NpCount => Some(s::ROW_NP_COUNT),
            Opt::HoldButtons => Some(s::ROW_HOLD_BUTTONS),
            _ => None,
        }
    }

    pub fn of_row(r: usize) -> Option<Opt> {
        Self::ALL.into_iter().find(|o| o.row() == Some(r))
    }
}

/// Which switches are on. `Default` is all off — the promise every opt-in makes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Opts(u32);

impl Opts {
    pub fn on(self, o: Opt) -> bool {
        self.0 & (1 << o as u32) != 0
    }
    pub fn set(&mut self, o: Opt, on: bool) {
        if on {
            self.0 |= 1 << o as u32;
        } else {
            self.0 &= !(1 << o as u32);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_switch_starts_off_and_has_its_own_key_and_row() {
        assert_eq!(
            Opt::ALL.len(),
            Opt::WifiRemote as usize + 1,
            "ALL lists every variant"
        );
        let none = Opts::default();
        for (i, o) in Opt::ALL.into_iter().enumerate() {
            assert!(!none.on(o), "{o:?} ships off");
            assert_eq!(Opt::from_key(o.key()), Some(o), "{o:?}'s key is its own");
            if let Some(r) = o.row() {
                assert_eq!(Opt::of_row(r), Some(o), "{o:?}'s row is its own");
            }
            let mut one = Opts::default();
            one.set(o, true);
            for (j, p) in Opt::ALL.into_iter().enumerate() {
                assert_eq!(one.on(p), i == j, "{o:?} on leaves {p:?} alone");
            }
            one.set(o, false);
            assert_eq!(one, none);
        }
    }
}
