//! nav — the navigation state machine that turns hardware button presses into screen
//! transitions + playback actions. Keymap-AGNOSTIC: it speaks logical `Button`s; the
//! backend (cinder-device / cinder-ffi) maps raw evdev `/dev/input/hoge` key codes to
//! these (that raw map needs on-device `getevent` calibration — it isn't in any extracted
//! DTB). `App` owns *navigation* state (which screen, cursor positions, theme); live
//! now-playing data is passed into `render` by the shell. `press` returns `Action`s the
//! shell performs via cinder-audio (PlayerService) — the UI never touches audio directly.

use crate::bluetooth::Bt;
use crate::library::{self, Tab};
use crate::menu::MenuItem;
use crate::model::Library;
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
    Album,
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
    EqChanged([i8; 10]), // shell applies the band gains to the sound DSP
    BtToggle(bool),      // shell turns the BT transmitter on/off
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
    lib_scroll: usize,
    lib_sort: usize,
    /// Album drill-in: the flat album index being viewed + the track cursor/scroll inside it.
    album_view: usize,
    album_track_idx: usize,
    album_track_scroll: usize,
    /// Hardware volume (0..VOL_MAX steps) + frames the volume HUD stays visible.
    volume: u8,
    vol_overlay: u8,
    /// Equalizer: 10 band gains (dB), selected band, active preset index.
    eq_bands: [i8; 10],
    eq_sel: usize,
    eq_preset: usize,
    /// Bluetooth on/off (transmit). The shell drives the radio + codec.
    bt_on: bool,
    /// The browsable library. Defaults to the design sample; the shell replaces it with the
    /// real DB contents via `set_library` after `cinder_db_open`.
    lib: Library,
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
            lib_scroll: 0,
            lib_sort: 0,
            album_view: 0,
            album_track_idx: 0,
            album_track_scroll: 0,
            volume: 18,
            vol_overlay: 0,
            eq_bands: data::EQ_PRESETS[3].1, // "A1"
            eq_sel: 0,
            eq_preset: 3,
            bt_on: true,
            lib: Library::sample(),
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
        library::row_count(self.lib_tab, &self.lib)
    }

    /// Replace the browsable library (called by the shell after the real DB is read). Resets
    /// the cursor so a stale index can't point past the new contents.
    pub fn set_library(&mut self, lib: Library) {
        self.lib = lib;
        self.lib_idx = 0;
        self.lib_scroll = 0;
    }

    /// Keep the library cursor inside the scrolled window (`library::PAGE` rows).
    fn lib_ensure_visible(&mut self) {
        if self.lib_idx < self.lib_scroll {
            self.lib_scroll = self.lib_idx;
        } else if self.lib_idx >= self.lib_scroll + library::PAGE {
            self.lib_scroll = self.lib_idx + 1 - library::PAGE;
        }
    }

    /// Resolve the currently-highlighted library row to a track object_id to play, if any.
    /// (Albums/Artists/Playlists return None for now — they need a drill-in; Songs play
    /// directly.) The shell turns this into a PlayerService play call.
    pub fn current_song_object_id(&self) -> Option<i64> {
        match self.lib_tab {
            Tab::Songs => self.lib.songs.get(self.lib_idx).map(|s| s.object_id),
            _ => None,
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
            Button::VolUp => {
                self.volume = (self.volume + 1).min(crate::overlay::VOL_MAX);
                self.vol_overlay = crate::overlay::VOL_FRAMES;
                return vec![Action::VolUp];
            }
            Button::VolDown => {
                self.volume = self.volume.saturating_sub(1);
                self.vol_overlay = crate::overlay::VOL_FRAMES;
                return vec![Action::VolDown];
            }
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
                    self.lib_scroll = 0;
                    vec![]
                }
                Button::Right => {
                    self.lib_tab = next_tab(self.lib_tab);
                    self.lib_idx = 0;
                    self.lib_scroll = 0;
                    vec![]
                }
                Button::Up => {
                    self.lib_idx = self.lib_idx.saturating_sub(1);
                    self.lib_ensure_visible();
                    vec![]
                }
                Button::Down => {
                    if self.lib_idx + 1 < self.lib_len() {
                        self.lib_idx += 1;
                        self.lib_ensure_visible();
                    }
                    vec![]
                }
                // Option cycles the Songs sort chip (TITLE / ARTIST / LENGTH).
                Button::Option if matches!(self.lib_tab, Tab::Songs) => {
                    self.lib_sort = (self.lib_sort + 1) % 3;
                    vec![]
                }
                Button::Select => match self.lib_tab {
                    // Albums drill into a track list; Songs/Playlists play the row directly.
                    Tab::Albums => {
                        self.album_view = self.lib_idx;
                        self.album_track_idx = 0;
                        self.album_track_scroll = 0;
                        self.push(Screen::Album);
                        vec![]
                    }
                    Tab::Songs => self
                        .lib
                        .songs
                        .get(self.lib_idx)
                        .map(|s| vec![Action::PlayIndex(s.object_id as usize)])
                        .unwrap_or_default(),
                    _ => vec![Action::PlayIndex(self.lib_idx)],
                },
                Button::Back => {
                    self.pop();
                    vec![]
                }
                _ => vec![],
            },
            Screen::Album => {
                let n = self
                    .lib
                    .albums_flat()
                    .get(self.album_view)
                    .map(|a| a.track_list.len())
                    .unwrap_or(0);
                match b {
                    Button::Up => {
                        self.album_track_idx = self.album_track_idx.saturating_sub(1);
                        if self.album_track_idx < self.album_track_scroll {
                            self.album_track_scroll = self.album_track_idx;
                        }
                        vec![]
                    }
                    Button::Down => {
                        if self.album_track_idx + 1 < n {
                            self.album_track_idx += 1;
                            if self.album_track_idx >= self.album_track_scroll + library::PAGE {
                                self.album_track_scroll = self.album_track_idx + 1 - library::PAGE;
                            }
                        }
                        vec![]
                    }
                    Button::Select => self
                        .lib
                        .albums_flat()
                        .get(self.album_view)
                        .and_then(|a| a.track_list.get(self.album_track_idx))
                        .map(|s| vec![Action::PlayIndex(s.object_id as usize)])
                        .unwrap_or_default(),
                    Button::Back | Button::Left => {
                        self.pop();
                        vec![]
                    }
                    _ => vec![],
                }
            }
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
            Screen::Eq => match b {
                Button::Left => {
                    self.eq_sel = self.eq_sel.saturating_sub(1);
                    vec![]
                }
                Button::Right => {
                    if self.eq_sel + 1 < self.eq_bands.len() {
                        self.eq_sel += 1;
                    }
                    vec![]
                }
                Button::Up => {
                    let g = &mut self.eq_bands[self.eq_sel];
                    *g = (*g + 1).min(6);
                    vec![Action::EqChanged(self.eq_bands)]
                }
                Button::Down => {
                    let g = &mut self.eq_bands[self.eq_sel];
                    *g = (*g - 1).max(-6);
                    vec![Action::EqChanged(self.eq_bands)]
                }
                // Option cycles presets (applies the preset's band gains).
                Button::Option | Button::Select => {
                    self.eq_preset = (self.eq_preset + 1) % data::EQ_PRESETS.len();
                    self.eq_bands = data::EQ_PRESETS[self.eq_preset].1;
                    vec![Action::EqChanged(self.eq_bands)]
                }
                Button::Back => {
                    self.pop();
                    vec![]
                }
                _ => vec![],
            },
            Screen::Bluetooth => match b {
                Button::Select => {
                    self.bt_on = !self.bt_on;
                    vec![Action::BtToggle(self.bt_on)]
                }
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
                // The Library row's caption reflects the real library size.
                let lib_value = if self.lib.is_empty() {
                    String::from("Empty")
                } else {
                    format!("{} albums · {} tracks", self.lib.album_count(), self.lib.songs.len())
                };
                let items: Vec<MenuItem> = MENU
                    .iter()
                    .enumerate()
                    .map(|(i, (screen, icon, label, value))| MenuItem {
                        icon,
                        label,
                        value: if *screen == Screen::Library { &lib_value } else { value },
                        active: i == self.menu_idx,
                    })
                    .collect();
                crate::menu::render(c, &theme, fonts, &items);
            }
            Screen::Library => crate::library::render(
                c, &theme, fonts, self.lib_tab, self.lib_idx, self.lib_scroll, self.lib_sort, &self.lib,
            ),
            Screen::Album => {
                let flat = self.lib.albums_flat();
                if let Some(al) = flat.get(self.album_view) {
                    crate::library::album_view(
                        c, &theme, fonts, al, self.album_track_idx, self.album_track_scroll,
                    );
                } else {
                    crate::library::render(
                        c, &theme, fonts, self.lib_tab, self.lib_idx, self.lib_scroll, self.lib_sort, &self.lib,
                    );
                }
            }
            Screen::UpNext => crate::up_next::render(c, &theme, fonts, 0),
            Screen::Eq => crate::eq::render(
                c, &theme, fonts, &self.eq_bands, data::EQ_PRESETS[self.eq_preset].0, self.eq_sel,
            ),
            Screen::Sound => crate::sound::render(c, &theme, fonts, &default_sound()),
            Screen::Bluetooth => {
                let bt = Bt {
                    on: self.bt_on,
                    connected: if self.bt_on { Some("WH-1000XM5") } else { None },
                    codec: "LDAC",
                };
                crate::bluetooth::render(c, &theme, fonts, &bt)
            }
            Screen::Settings => crate::settings::render(c, &theme, fonts, self.night, false),
            Screen::Fm => crate::fm::render(c, &theme, fonts, 88.6),
            Screen::UsbDac => crate::usbdac::render(c, &theme, fonts, false, "A1", true),
            Screen::Receiver => crate::receiver::render(c, &theme, fonts, false),
        }
        // Transient HUD on top of any screen (except the lock screen, which owns the panel).
        if self.vol_overlay > 0 && self.current() != Screen::Lock {
            crate::overlay::volume(c, &theme, fonts, self.volume);
        }
    }

    /// Advance per-frame timers (overlay countdowns). The shell calls this once per pump tick
    /// before `render`. Returns true while something time-driven still needs redrawing.
    pub fn tick(&mut self) -> bool {
        if self.vol_overlay > 0 {
            self.vol_overlay -= 1;
        }
        self.vol_overlay > 0
    }

    /// Sync the UI volume to the device's real level (the shell pushes this after it reads/sets
    /// PlayerService volume), without popping the HUD.
    pub fn set_volume(&mut self, level: u8) {
        self.volume = level.min(crate::overlay::VOL_MAX);
    }

    pub fn volume(&self) -> u8 {
        self.volume
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
    fn volume_keys_adjust_and_pop_hud() {
        let mut a = unlocked();
        let v0 = a.volume;
        assert_eq!(a.press(Button::VolUp), vec![Action::VolUp]);
        assert_eq!(a.volume, v0 + 1);
        assert!(a.vol_overlay > 0); // HUD showing
                                    // HUD counts down and eventually hides
        for _ in 0..crate::overlay::VOL_FRAMES {
            a.tick();
        }
        assert_eq!(a.vol_overlay, 0);
        // never below 0
        a.set_volume(0);
        a.press(Button::VolDown);
        assert_eq!(a.volume, 0);
    }

    fn enter_eq() -> App {
        let mut a = unlocked();
        a.press(Button::Up); // Menu
        for _ in 0..4 {
            a.press(Button::Down);
        }
        a.press(Button::Select); // Equalizer (idx 4)
        assert_eq!(a.current(), Screen::Eq);
        a
    }

    #[test]
    fn eq_band_select_and_adjust() {
        let mut a = enter_eq();
        a.press(Button::Right); // band 1
        let before = a.eq_bands[1];
        let acts = a.press(Button::Up);
        assert_eq!(a.eq_bands[1], (before + 1).min(6));
        assert!(matches!(acts.as_slice(), [Action::EqChanged(_)]));
        // clamps at +6 / -6
        for _ in 0..20 {
            a.press(Button::Up);
        }
        assert_eq!(a.eq_bands[1], 6);
        for _ in 0..20 {
            a.press(Button::Down);
        }
        assert_eq!(a.eq_bands[1], -6);
    }

    #[test]
    fn bluetooth_select_toggles() {
        let mut a = unlocked();
        a.press(Button::Up); // Menu
        for _ in 0..6 {
            a.press(Button::Down);
        }
        a.press(Button::Select); // Bluetooth (idx 6)
        assert_eq!(a.current(), Screen::Bluetooth);
        let was = a.bt_on;
        let acts = a.press(Button::Select);
        assert_eq!(acts, vec![Action::BtToggle(!was)]);
        assert_eq!(a.bt_on, !was);
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
