//! nav — the navigation state machine that turns hardware button presses into screen
//! transitions + playback actions. Keymap-AGNOSTIC: it speaks logical `Button`s; the
//! backend (cinder-device / cinder-ffi) maps raw evdev `/dev/input/hoge` key codes to
//! these (that raw map needs on-device `getevent` calibration — it isn't in any extracted
//! DTB). `App` owns *navigation* state (which screen, cursor positions, theme); live
//! now-playing data is passed into `render` by the shell. `press` returns `Action`s the
//! shell performs via cinder-audio (PlayerService) — the UI never touches audio directly.

use crate::bluetooth::Bt;
use crate::library::Tab;
use crate::menu::MenuItem;
use crate::now_playing::NowPlaying;
use crate::sound::Sound;
use crate::{data, Canvas, FontSet, Theme};

/// Logical buttons (the physical NW-A50 keys, mapped from raw codes by the backend).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Button {
    Up,
    Down,
    Left,
    Right,
    Select,
    Back,
    Option,
    Play,
    Home,
    VolUp,
    VolDown,
    Power,
}

/// The screens the navigator can be on (the route-stack entries).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Screen {
    Lock,
    NowPlaying,
    Menu,
    Library,
    UpNext,
    Eq,
    Sound,
    Bluetooth,
    Settings,
    Fm,
    UsbDac,
    Receiver,
}

/// Side effects the shell carries out (via cinder-audio / system services). The UI emits
/// these instead of acting on audio itself.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    PlayPause,
    Next,
    Prev,
    NextAlbum,
    PrevAlbum,
    VolUp,
    VolDown,
    PlayIndex(usize), // play the highlighted library item (meaning is tab-dependent)
    ThemeChanged(bool),
    Sleep,
    EnterUsbMsc,
}

/// The Menu rows, in display order — index ↔ destination Screen.
const MENU: [(Screen, &str, &str, &str); 10] = [
    (Screen::NowPlaying, "note", "Now Playing", ""),
    (Screen::Library, "library", "Library", "124 albums · 1,842 tracks"),
    (Screen::UpNext, "queue", "Up Next", "8 tracks · 41:24"),
    (Screen::Fm, "radio", "FM Radio", "88.6 MHz"),
    (Screen::Eq, "eq", "Equalizer", "Custom A1"),
    (Screen::Sound, "sound", "Sound Settings", "DSEE HX · VPT · Vinyl"),
    (Screen::Bluetooth, "bt", "Bluetooth", "WH-1000XM5 · LDAC"),
    (Screen::UsbDac, "usb", "USB-DAC", "Off"),
    (Screen::Receiver, "rx", "BT Receiver", "Off"),
    (Screen::Settings, "settings", "Settings", "System · Storage · About"),
];

pub struct App {
    stack: Vec<Screen>,
    pub night: bool,
    pub locked: bool,
    pub playing: bool,
    menu_idx: usize,
    lib_tab: Tab,
    lib_idx: usize,
}

impl Default for App {
    fn default() -> Self {
        App {
            stack: vec![Screen::Lock],
            night: false,
            locked: true,
            playing: true,
            menu_idx: 0,
            lib_tab: Tab::Albums,
            lib_idx: 0,
        }
    }
}

impl App {
    pub fn new() -> Self {
        Self::default()
    }

    /// Start unlocked on Now Playing (used by the shell for first-boot bring-up, where the
    /// lock screen would just get in the way of confirming the panel paints).
    pub fn unlocked() -> Self {
        App { stack: vec![Screen::NowPlaying], locked: false, ..Self::default() }
    }

    /// The screen currently on top of the route stack.
    pub fn current(&self) -> Screen {
        *self.stack.last().unwrap_or(&Screen::NowPlaying)
    }

    pub fn menu_index(&self) -> usize {
        self.menu_idx
    }
    pub fn lib_tab(&self) -> Tab {
        self.lib_tab
    }
    pub fn lib_index(&self) -> usize {
        self.lib_idx
    }

    fn push(&mut self, s: Screen) {
        if self.current() != s {
            self.stack.push(s);
        }
    }
    fn pop(&mut self) {
        if self.stack.len() > 1 {
            self.stack.pop();
        }
    }
    fn go(&mut self, s: Screen) {
        self.stack = vec![s];
    }

    /// Number of rows in the current library tab (for cursor clamping).
    fn lib_len(&self) -> usize {
        match self.lib_tab {
            Tab::Songs => data::SONGS.len(),
            Tab::Albums => data::ALBUM_GROUPS.len(),
            Tab::Artists => data::ARTISTS.len(),
            Tab::Playlists => data::PLAYLISTS.len(),
        }
    }

    /// Handle a button press; returns any actions for the shell to perform.
    pub fn press(&mut self, b: Button) -> Vec<Action> {
        // Locked: any key wakes to Now Playing (except Power, which just confirms sleep).
        if self.locked {
            return match b {
                Button::Power => vec![Action::Sleep],
                _ => {
                    self.locked = false;
                    self.go(Screen::NowPlaying);
                    vec![]
                }
            };
        }

        // Global gestures, available on every screen.
        match b {
            Button::Power => {
                self.locked = true;
                self.go(Screen::Lock);
                return vec![Action::Sleep];
            }
            Button::Home => {
                self.go(Screen::NowPlaying);
                return vec![];
            }
            Button::VolUp => return vec![Action::VolUp],
            Button::VolDown => return vec![Action::VolDown],
            Button::Play => {
                // Play/pause is global on a music player.
                self.playing = !self.playing;
                return vec![Action::PlayPause];
            }
            _ => {}
        }

        match self.current() {
            Screen::NowPlaying => match b {
                Button::Right => vec![Action::Next],
                Button::Left => vec![Action::Prev],
                Button::Up | Button::Select | Button::Option => {
                    self.push(Screen::Menu);
                    vec![]
                }
                Button::Down => {
                    self.push(Screen::UpNext);
                    vec![]
                }
                _ => vec![],
            },
            Screen::Menu => match b {
                Button::Up => {
                    self.menu_idx = self.menu_idx.saturating_sub(1);
                    vec![]
                }
                Button::Down => {
                    if self.menu_idx + 1 < MENU.len() {
                        self.menu_idx += 1;
                    }
                    vec![]
                }
                Button::Select | Button::Right => {
                    let target = MENU[self.menu_idx].0;
                    if target == Screen::NowPlaying {
                        self.go(Screen::NowPlaying);
                    } else {
                        self.push(target);
                    }
                    vec![]
                }
                Button::Back | Button::Left => {
                    self.pop();
                    vec![]
                }
                _ => vec![],
            },
            Screen::Library => match b {
                Button::Left => {
                    self.lib_tab = prev_tab(self.lib_tab);
                    self.lib_idx = 0;
                    vec![]
                }
                Button::Right => {
                    self.lib_tab = next_tab(self.lib_tab);
                    self.lib_idx = 0;
                    vec![]
                }
                Button::Up => {
                    self.lib_idx = self.lib_idx.saturating_sub(1);
                    vec![]
                }
                Button::Down => {
                    if self.lib_idx + 1 < self.lib_len() {
                        self.lib_idx += 1;
                    }
                    vec![]
                }
                Button::Select => vec![Action::PlayIndex(self.lib_idx)],
                Button::Back => {
                    self.pop();
                    vec![]
                }
                _ => vec![],
            },
            Screen::Settings => match b {
                // Minimal: Select toggles day/night (the most-used setting) for now.
                Button::Select => {
                    self.night = !self.night;
                    vec![Action::ThemeChanged(self.night)]
                }
                Button::Back => {
                    self.pop();
                    vec![]
                }
                _ => vec![],
            },
            Screen::UsbDac => match b {
                Button::Select => vec![Action::EnterUsbMsc],
                Button::Back => {
                    self.pop();
                    vec![]
                }
                _ => vec![],
            },
            // The remaining screens (Eq/Sound/Bluetooth/Fm/Receiver/UpNext): Back pops,
            // everything else is a no-op until their per-screen controls are wired.
            _ => match b {
                Button::Back => {
                    self.pop();
                    vec![]
                }
                _ => vec![],
            },
        }
    }

    /// Draw the current screen. Live now-playing data comes from the shell (`np`); list/
    /// settings screens currently use the design sample data (real data wires in later).
    pub fn render(&self, c: &mut Canvas, fonts: &FontSet, np: &NowPlaying) {
        let theme = if self.night { Theme::night() } else { Theme::day() };
        match self.current() {
            Screen::Lock => {
                let lk = crate::lock::Lock {
                    clock: np.clock,
                    big_clock: np.clock,
                    title: np.title,
                    artist: np.artist,
                    badge: np.badge,
                    battery: np.battery,
                    progress: np.progress,
                };
                crate::lock::render(c, &theme, fonts, &lk);
            }
            Screen::NowPlaying => crate::now_playing::render(c, &theme, fonts, np),
            Screen::Menu => {
                let items: Vec<MenuItem> = MENU
                    .iter()
                    .enumerate()
                    .map(|(i, (_, icon, label, value))| MenuItem {
                        icon,
                        label,
                        value,
                        active: i == self.menu_idx,
                    })
                    .collect();
                crate::menu::render(c, &theme, fonts, &items);
            }
            Screen::Library => {
                crate::library::render(c, &theme, fonts, self.lib_tab, self.lib_idx, 0)
            }
            Screen::UpNext => crate::up_next::render(c, &theme, fonts, 0),
            Screen::Eq => crate::eq::render(c, &theme, fonts, &data::EQ_PRESETS[0].1, "A1"),
            Screen::Sound => crate::sound::render(c, &theme, fonts, &default_sound()),
            Screen::Bluetooth => crate::bluetooth::render(c, &theme, fonts, &default_bt()),
            Screen::Settings => crate::settings::render(c, &theme, fonts, self.night, false),
            Screen::Fm => crate::fm::render(c, &theme, fonts, 88.6),
            Screen::UsbDac => crate::usbdac::render(c, &theme, fonts, false, "A1", true),
            Screen::Receiver => crate::receiver::render(c, &theme, fonts, false),
        }
    }
}

fn next_tab(t: Tab) -> Tab {
    match t {
        Tab::Songs => Tab::Albums,
        Tab::Albums => Tab::Artists,
        Tab::Artists => Tab::Playlists,
        Tab::Playlists => Tab::Songs,
    }
}
fn prev_tab(t: Tab) -> Tab {
    match t {
        Tab::Songs => Tab::Playlists,
        Tab::Albums => Tab::Songs,
        Tab::Artists => Tab::Albums,
        Tab::Playlists => Tab::Artists,
    }
}

fn default_sound() -> Sound {
    Sound {
        dsee: true,
        vinyl: false,
        vpt: "Studio",
        dcphase: "Low A",
        normalizer: true,
        clearaudio: false,
        eq_preset: "A1",
        bt_codec: Some("LDAC"),
    }
}
fn default_bt() -> Bt {
    Bt { on: true, connected: Some("WH-1000XM5"), codec: "LDAC" }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unlocked() -> App {
        let mut a = App::new();
        a.press(Button::Select); // wake from Lock
        a
    }

    #[test]
    fn boots_locked_and_any_key_wakes() {
        let mut a = App::new();
        assert_eq!(a.current(), Screen::Lock);
        assert!(a.locked);
        let acts = a.press(Button::Down);
        assert_eq!(a.current(), Screen::NowPlaying);
        assert!(!a.locked);
        assert!(acts.is_empty());
    }

    #[test]
    fn power_sleeps_and_locks() {
        let mut a = unlocked();
        let acts = a.press(Button::Power);
        assert_eq!(acts, vec![Action::Sleep]);
        assert!(a.locked);
        assert_eq!(a.current(), Screen::Lock);
    }

    #[test]
    fn now_playing_transport_emits_actions() {
        let mut a = unlocked();
        assert_eq!(a.press(Button::Right), vec![Action::Next]);
        assert_eq!(a.press(Button::Left), vec![Action::Prev]);
        let before = a.playing;
        assert_eq!(a.press(Button::Play), vec![Action::PlayPause]);
        assert_eq!(a.playing, !before);
    }

    #[test]
    fn menu_navigation_and_routing() {
        let mut a = unlocked();
        a.press(Button::Up); // NowPlaying -> Menu
        assert_eq!(a.current(), Screen::Menu);
        // move to "Library" (index 1) and select
        a.press(Button::Down);
        assert_eq!(a.menu_index(), 1);
        a.press(Button::Select);
        assert_eq!(a.current(), Screen::Library);
        // Back returns to Menu, Back again to NowPlaying
        a.press(Button::Back);
        assert_eq!(a.current(), Screen::Menu);
        a.press(Button::Back);
        assert_eq!(a.current(), Screen::NowPlaying);
    }

    #[test]
    fn menu_cursor_clamps() {
        let mut a = unlocked();
        a.press(Button::Up); // -> Menu
        for _ in 0..3 {
            a.press(Button::Up);
        }
        assert_eq!(a.menu_index(), 0); // never below 0
        for _ in 0..50 {
            a.press(Button::Down);
        }
        assert_eq!(a.menu_index(), MENU.len() - 1); // never past end
    }

    #[test]
    fn library_tabs_and_cursor() {
        let mut a = unlocked();
        a.press(Button::Up); // Menu
        a.press(Button::Down); // -> Library row
        a.press(Button::Select); // enter Library
        assert_eq!(a.current(), Screen::Library);
        let start = a.lib_tab();
        a.press(Button::Right);
        assert_ne!(a.lib_tab(), start); // tab changed
        assert_eq!(a.lib_index(), 0); // cursor reset on tab change
        a.press(Button::Down);
        assert!(a.lib_index() <= 1);
        // Select on a library row asks the shell to play that index
        let acts = a.press(Button::Select);
        assert!(matches!(acts.as_slice(), [Action::PlayIndex(_)]));
    }

    #[test]
    fn home_resets_to_now_playing() {
        let mut a = unlocked();
        a.press(Button::Up); // Menu
        a.press(Button::Down);
        a.press(Button::Select); // Library
        a.press(Button::Home);
        assert_eq!(a.current(), Screen::NowPlaying);
    }

    #[test]
    fn settings_select_toggles_theme() {
        let mut a = unlocked();
        // route to Settings (last menu item, index 9)
        a.press(Button::Up); // Menu
        for _ in 0..9 {
            a.press(Button::Down);
        }
        assert_eq!(a.menu_index(), 9);
        a.press(Button::Select);
        assert_eq!(a.current(), Screen::Settings);
        let was = a.night;
        let acts = a.press(Button::Select);
        assert_eq!(acts, vec![Action::ThemeChanged(!was)]);
        assert_eq!(a.night, !was);
    }
}
