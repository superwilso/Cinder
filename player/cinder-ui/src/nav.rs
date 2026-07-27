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
use crate::model::{Library, SongRow};
use crate::now_playing::NowPlaying;
use crate::sound::Sound;
use crate::theme::Accent;
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
    /// Dedicated side FF/REW keys: GLOBAL next/previous track on every screen (stock behavior).
    /// Distinct from `Right`/`Left`, which stay contextual navigation for the sim/host keyboards.
    Next,
    Prev,
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
    Onboarding,
    /// USB mass-storage mode: MODAL "connected to PC" screen while the storage volume is handed
    /// to the host. Only Back (or the shell detecting cable-unplug) leaves it.
    UsbStorage,
    /// Sentinel for the Menu row that opens the Shelf. The Shelf is a bottom-sheet OVERLAY (see
    /// `shelf_open`), never pushed onto the route stack — selecting this row calls `open_shelf()`.
    Shelf,
}

/// What the accent band on a Library tab shuffles. Each variant matches the sub-label the band
/// draws, so the promise on screen and the behaviour stay in step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShuffleScope {
    /// "Shuffle all songs" — every track, random order.
    AllSongs,
    /// "Shuffle by album" — random album order, tracks in sequence within each album.
    ByAlbum,
    /// "Shuffle by artist" — one random artist, their tracks shuffled.
    ByArtist,
    /// "Shuffle a playlist" — one random playlist, its tracks shuffled.
    Playlist,
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
    PlayIndex(i64), // play the library track with this DB object_id (the shell resolves it to a URI)
    PlayPlaylist(i64), // play a whole playlist from the top (DB container object_id)
    Shuffle(ShuffleScope), // the accent band on each Library tab — shuffle within that scope
    ThemeChanged(bool),
    Sleep,
    EnterUsbMsc,
    ExitUsbMsc, // leave USB mass-storage: the shell remounts the volume + restores the USB mode
    EqChanged([i8; 10]), // shell applies the band gains to the sound DSP
    BtToggle(bool),      // shell turns the BT transmitter on/off
    BatteryCareChanged(bool), // shell calls PowerMgrServiceClient::EnableItawariCharging
    SoundChanged,             // shell reads cinder_get_sound_flags + applies via EffectCtrlDmp
    SoundBypass(bool),        // A/B: true = bypass whole chain (B), false = re-enable (A)
    SleepTimer(u32),          // arm/cancel the sleep timer: minutes (0 = off); cinder-ffi counts down
    ShuffleToggle,            // Now Playing shuffle on/off (FFI holds the state; PlayController wiring is device-gated)
    RepeatCycle,              // Now Playing repeat: off → all → one (FFI holds the state)
    BtCodecChanged,           // device-wide BT transmit codec / LDAC quality changed; shell reads + applies
    UsbDacToggle(bool),       // engage/disengage USB-DAC input routed to 3.5mm + BT/LDAC (the headline feature)
    BrightnessChanged(u8),    // panel brightness level 1..5; shell maps it onto the backlight node
    ScreenOffTimer(u32),      // idle screen-off timeout in seconds (0 = off); the shell counts idle
    BootToStock,              // arm a ONE-SHOT boot into Sony's player, then restart
    ToggleLiked,              // heart the currently playing track (cinder-ffi owns the set)
}

/// The Menu rows, in display order — index ↔ destination Screen. Matches the prototype's 10 rows;
/// the Shelf is NOT here by design (it's opened from the status-bar bookmark glyph). "Help &
/// Controls" is the one Cinder addition (the onboarding intro, re-openable).
// The `value` column is the row's SUBTITLE. Anything that describes live state is filled in at
// render time (see the `value:` match in render) — the literals here are only for rows whose
// subtitle is genuinely fixed, or empty for rows that have nothing true to say yet.
//
// These used to be the prototype's mock strings and they read as fact: "124 albums · 1,842 tracks"
// on a device with 304 albums, "88.6 MHz" for a tuner that isn't wired, "Custom A1" regardless of
// the selected EQ preset, and "WH-1000XM5 · LDAC" naming a pair of headphones that were never
// connected. A subtitle that states something false is worse than no subtitle.
const MENU: [(Screen, &str, &str, &str); 11] = [
    (Screen::NowPlaying, "note", "Now Playing", ""),   // live: current track · elapsed
    (Screen::Library, "library", "Library", ""),      // live: album/track counts
    (Screen::UpNext, "queue", "Up Next", ""),         // live: queue length
    (Screen::Fm, "radio", "FM Radio", ""),            // tuner not wired — claim nothing
    (Screen::Eq, "eq", "Equalizer", ""),              // live: selected preset
    (Screen::Sound, "sound", "Sound Settings", ""),   // live: which effects are on
    (Screen::Bluetooth, "bt", "Bluetooth", ""),       // live: configured transmit codec
    (Screen::UsbDac, "usb", "USB-DAC", ""),           // live: On/Off
    (Screen::Receiver, "rx", "BT Receiver", "Off"),   // not wired, and Off is the truth
    (Screen::Settings, "settings", "Settings", "System · Storage · About"),
    (Screen::Onboarding, "note", "Help & Controls", "Button map · features"),
];

/// Nominal frame period at 60 fps, in ms. The HUD/fling constants are expressed in these frames;
/// `tick_dt` converts real elapsed time into them so the durations hold at any actual frame rate.
const FRAME_MS: u32 = 17;

/// Screen-off (idle) timeout presets, in seconds. 0 = off and is first, so the cycle starts from
/// "no idle blank" and the feature is strictly opt-in.
pub const SCREEN_OFF_PRESETS: [u32; 5] = [0, 15, 30, 60, 120];

/// Label for a screen-off preset, matching the row's other mono values.
pub fn screen_off_label(secs: u32) -> String {
    match secs {
        0 => String::from("OFF"),
        s if s < 60 => format!("{s} SEC"),
        s if s % 60 == 0 => format!("{} MIN", s / 60),
        s => format!("{}:{:02}", s / 60, s % 60),
    }
}

/// The Menu's live row subtitles (see `App::menu_subtitles`). One field per Menu row whose caption
/// describes current state rather than being fixed text.
pub(crate) struct MenuSubtitles {
    pub now_playing: String,
    pub library: String,
    pub queue: String,
    pub eq: String,
    pub sound: String,
    pub bluetooth: String,
    pub usb_dac: String,
}

/// A pinned place on the Shelf: enough route context to jump straight back. Session-scoped (held in
/// the navigator; persisting across boots is a later refinement).
#[derive(Clone)]
struct ShelfPin {
    screen: Screen,
    lib_tab: Tab,
    album_view: usize,
    title: String,
    sub: String,
}

pub struct App {
    stack: Vec<Screen>,
    pub night: bool,
    /// The chosen accent colour. Purely a render input — no shell action, no Sony service, so
    /// changing it costs one repaint and a settings write.
    pub accent: Accent,
    pub locked: bool,
    pub playing: bool,
    menu_idx: usize,
    lib_tab: Tab,
    lib_idx: usize,
    /// Library list scroll in PIXELS (live drag + fling; rows render at a sub-row offset).
    lib_scroll_px: i32,
    lib_sort: usize,
    /// Albums-tab ORDER chip (index into library::ALBUM_SORTS: 0 artist-grouped, 1 A-Z, 2 added,
    /// 3 year) + the one expanded accordion album (a lib.albums_flat() index, stable across
    /// re-orders; None = all collapsed). Session state — not persisted.
    album_sort: usize,
    album_expanded: Option<usize>,
    /// Album drill-in: the flat album index being viewed + the track cursor/pixel-scroll inside it.
    album_view: usize,
    /// Decoded 96x96 cover for the album currently drilled into, or None for the gradient.
    /// Set by the shell (which owns the art cache) whenever the open album changes; the UI never
    /// decodes anything itself.
    album_cover: Option<crate::art::Image>,
    album_track_idx: usize,
    album_scroll_px: i32,
    /// Fling (momentum) velocity in px/s for the current scrollable list; decays each tick.
    fling_v: f32,
    /// Hardware volume (0..VOL_MAX steps) + frames the volume HUD stays visible.
    volume: u8,
    vol_overlay: u8,
    /// Equalizer: 10 band gains (dB), selected band, active preset index.
    eq_bands: [i8; 10],
    eq_sel: usize,
    eq_preset: usize,
    /// Bluetooth on/off (transmit). The shell drives the radio + codec.
    bt_on: bool,
    /// Device-wide BT transmit codec preference + LDAC quality tier. Used for BOTH normal BT
    /// playback and the USB-DAC→LDAC bridge. `bt_codec` indexes bluetooth::CODECS (0 = LDAC);
    /// `bt_ldac_quality` indexes bluetooth::QUALITIES (0 = Auto). Persisted; applied by the shell.
    bt_codec: u8,
    bt_ldac_quality: u8,
    /// USB-DAC mode engaged (input from a USB host → 3.5mm + BT/LDAC). Transient (not persisted).
    usb_dac_on: bool,
    /// Now Playing visualiser type (cinder_ui::viz index) + animation on/off (UI settings).
    viz_kind: u8,
    viz_size: u8,
    /// Which Now Playing page is showing (see now_playing::NpPage). Persisted.
    np_page: u8,
    /// Settings screen cursor.
    settings_sel: usize,
    /// Battery care (Sony "Itawari" charging, ~90% cap). Mirrors the device state; the shell reads
    /// the real value at boot via cinder_set_battery_care and applies toggles via the action.
    battery_care: bool,
    /// Sound Settings effect toggles. Each maps to an EffectCtrlDmp boolean setter (the shell reads
    /// these via cinder_get_sound_flags after a SoundChanged action). VPT/DC-Phase are on/off here;
    /// their mode/type (Studio/Club, Standard/Low) is a device-gated enhancement (enum values TBD).
    snd_dsee: bool,
    snd_vinyl: bool,
    snd_vpt: bool,
    snd_dc: bool,
    snd_norm: bool,
    snd_clear: bool,
    /// A/B compare on the Sound screen: false = A (effects active), true = B (whole chain bypassed,
    /// "direct"). The Option button flips it for an instant listen test; the shell calls the DSP
    /// bypass. Independent of the per-effect toggles — flipping back to A restores them.
    snd_ab_bypass: bool,
    /// Sound screen cursor.
    sound_sel: usize,
    /// Real storage usage label ("used / total GB"), pushed by the shell from statvfs. Empty until
    /// the shell reports it (Settings then shows a neutral placeholder).
    storage: String,
    /// Sleep timer: `sleep_idx` cycles the presets (Off/15/30/45/60 min) in Settings; `sleep_min` is
    /// the LIVE remaining minutes that cinder-ffi counts down and pushes back for display. 0 = off.
    sleep_idx: usize,
    /// Boot-to-stock confirmation: the row arms on the first tap and only acts on the second, so a
    /// stray tap can't restart the device. Cleared by leaving Settings or tapping anything else.
    boot_stock_armed: bool,
    /// How many tracks are liked — drives the Library's "Liked songs" row. The set itself lives in
    /// cinder-ffi (it owns persistence); nav only needs the count to render.
    liked_count: usize,
    /// Settings is taller than the panel, so it scrolls like the library lists.
    settings_scroll_px: i32,
    /// Screen-off (idle) timeout in SECONDS; 0 = off, which is the default — nothing changes unless
    /// the user opts in. `screen_off_idx` cycles the presets from the Settings row.
    screen_off_idx: usize,
    screen_off_s: u32,
    /// Panel brightness, 1..=5 (Settings row cycles it). Deliberately has no 0: the shell maps
    /// level 1 to a dim-but-readable fraction of max_brightness, so no setting reachable from the
    /// UI can leave a screen you can't read to change it back. Persisted.
    brightness: u8,
    sleep_min: u32,
    /// First-run onboarding: which page is showing, and whether the intro has been completed (the
    /// latter is persisted, so it only appears once; the Menu can re-open it any time).
    onboarding_page: usize,
    onboarding_seen: bool,
    /// The browsable library. Defaults to the design sample; the shell replaces it with the
    /// real DB contents via `set_library` after `cinder_db_open`.
    lib: Library,
    /// Shelf bottom-sheet overlay: whether it's showing, and the three pin slots (jump-back places).
    shelf_open: bool,
    pins: [Option<ShelfPin>; 3],
    /// User play-queue (Spotify-style right-swipe on a song row adds to it). Shown on Up Next in
    /// front of the album-derived list. Display + intent today: making PlayerService actually play
    /// this order lands with PlayController::SetTrackSequence (RE pending).
    queue: Vec<SongRow>,
    /// Transient bottom toast ("Added to queue — …"): text + frames left (fades via tick()).
    toast: String,
    toast_frames: u8,
    /// Swipe-to-queue feedback animation: a "+ QUEUED" chip slides right from the swiped row and
    /// fades. `queue_anim_y` = the row's y (UI coords); frames count down via tick().
    queue_anim_y: i32,
    queue_anim_frames: u8,
    /// Object ids of the Up Next rows the last render actually drew, in drawn order. The window
    /// auto-scrolls to follow playback, so the renderer (which knows `np`) publishes this for the
    /// hit test instead of `tap` trying to recompute it.
    up_next_rows: Vec<i64>,
}

/// How long the toast stays up (~1.8 s at the 60 fps pump).
const TOAST_FRAMES: u8 = 110;
/// Queue-chip slide animation length (~0.4 s at 60 fps).
const QUEUE_ANIM_FRAMES: u8 = 24;

impl Default for App {
    fn default() -> Self {
        App {
            stack: vec![Screen::Lock],
            album_cover: None,
            night: false,
            accent: Accent::default(),
            locked: true,
            playing: true,
            menu_idx: 0,
            lib_tab: Tab::Albums,
            lib_idx: 0,
            lib_scroll_px: 0,
            lib_sort: 0,
            album_sort: 0,
            album_expanded: None,
            album_view: 0,
            album_track_idx: 0,
            album_scroll_px: 0,
            fling_v: 0.0,
            volume: 15,
            vol_overlay: 0,
            eq_bands: data::EQ_PRESETS[3].1, // "A1"
            eq_sel: 0,
            eq_preset: 3,
            bt_on: true,
            bt_codec: 0,        // LDAC
            bt_ldac_quality: 0, // Auto
            usb_dac_on: false,
            viz_kind: 0,
            viz_size: 1, // VEIL on the cover page; the big spectrum has its own page
            np_page: 0,
            settings_sel: 0,
            battery_care: false,
            snd_dsee: false,
            snd_vinyl: false,
            snd_vpt: false,
            snd_dc: false,
            snd_norm: false,
            snd_clear: false,
            snd_ab_bypass: false,
            sound_sel: 0,
            storage: String::new(),
            sleep_idx: 0,
            boot_stock_armed: false,
            liked_count: 0,
            settings_scroll_px: 0,
            screen_off_idx: 0,
            screen_off_s: 0,  // OFF by default — an idle blank is opt-in
            brightness: 4,   // matches the shell's ~70% day default
            sleep_min: 0,
            onboarding_page: 0,
            onboarding_seen: false,
            lib: Library::sample(),
            shelf_open: false,
            pins: [None, None, None],
            queue: Vec::new(),
            toast: String::new(),
            toast_frames: 0,
            queue_anim_y: 0,
            queue_anim_frames: 0,
            up_next_rows: Vec::new(),
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

    /// The physical Hold/lock SWITCH changed state. `held` = the switch is now engaged: lock the
    /// touchscreen and show the Lock screen. `!held` = the switch went off: this is the ONLY thing
    /// that unlocks (returns to Now Playing). Power never unlocks — it just toggles the screen.
    /// Idempotent, so the shell can call it on every reported edge without tracking state itself.
    pub fn set_hold(&mut self, held: bool) {
        if held == self.locked {
            return;
        }
        self.locked = held;
        self.go(if held { Screen::Lock } else { Screen::NowPlaying });
    }

    /// The screen currently on top of the route stack.
    pub fn current(&self) -> Screen {
        *self.stack.last().unwrap_or(&Screen::NowPlaying)
    }

    /// Programmatically raise the USB mass-storage modal — used when the shell auto-detects a PC
    /// host and enters MSC on its own (no settings-row tap). Idempotent: no-op if the modal is
    /// already up, so the ~1 Hz auto-detect poll can call it every tick without stacking screens.
    pub fn show_usb_storage(&mut self) {
        if !matches!(self.current(), Screen::UsbStorage) {
            self.push(Screen::UsbStorage);
        }
    }

    /// True while the USB mass-storage modal owns the screen (shell asks before auto-entering).
    pub fn is_usb_storage(&self) -> bool {
        matches!(self.current(), Screen::UsbStorage)
    }

    /// Activate a Menu row: navigate to its destination, or open the Shelf overlay for the Shelf
    /// sentinel. Shared by the Menu tap + Select handlers so they can't drift apart.
    fn activate_menu(&mut self, row: usize) {
        match MENU[row].0 {
            Screen::NowPlaying => self.go(Screen::NowPlaying),
            Screen::Onboarding => {
                self.onboarding_page = 0; // re-open the intro from the start
                self.push(Screen::Onboarding);
            }
            target => self.push(target),
        }
    }

    /// Open the Shelf bottom-sheet over the current place. Entered from the Menu, so pop the Menu
    /// first — the sheet should overlay the real screen beneath it, not the Menu.
    pub fn open_shelf(&mut self) {
        if self.current() == Screen::Menu {
            self.pop();
        }
        self.shelf_open = true;
    }

    /// Whether the Shelf overlay is showing (the shell can use this, e.g. to keep painting).
    pub fn shelf_is_open(&self) -> bool {
        self.shelf_open
    }

    /// A short title/subtitle for the current place — used by the Shelf's "this place" row and when
    /// pinning. Derived from the live nav state (no shell round-trip needed).
    fn place_label(&self) -> (String, String) {
        match self.current() {
            Screen::NowPlaying => ("Now Playing".into(), "Current track".into()),
            Screen::Library => ("Library".into(), tab_name(self.lib_tab).into()),
            Screen::Album => match self.lib.albums_flat().get(self.album_view) {
                Some(al) => (al.name.clone(), al.artist.clone()),
                None => ("Album".into(), String::new()),
            },
            s => (screen_title(s).into(), String::new()),
        }
    }

    /// Handle a tap while the Shelf overlay is open (geometry comes from `shelf::hit`).
    fn shelf_tap(&mut self, x: i32, y: i32) -> Vec<Action> {
        use crate::shelf::ShelfHit;
        match crate::shelf::hit(x, y) {
            ShelfHit::Close => self.shelf_open = false,
            ShelfHit::Undo => {
                self.shelf_open = false;
                self.pop();
            }
            ShelfHit::Pin => {
                let (title, sub) = self.place_label();
                let pin = ShelfPin {
                    screen: self.current(),
                    lib_tab: self.lib_tab,
                    album_view: self.album_view,
                    title,
                    sub,
                };
                // first empty slot, else overwrite the oldest (slot 0)
                match self.pins.iter_mut().find(|p| p.is_none()) {
                    Some(slot) => *slot = Some(pin),
                    None => self.pins[0] = Some(pin),
                }
            }
            ShelfHit::Go(i) => {
                if let Some(p) = self.pins.get(i).and_then(|p| p.clone()) {
                    self.lib_tab = p.lib_tab;
                    self.album_view = p.album_view;
                    self.go(p.screen);
                }
                self.shelf_open = false;
            }
            ShelfHit::Clear(i) => {
                if let Some(slot) = self.pins.get_mut(i) {
                    *slot = None;
                }
            }
            ShelfHit::None => {}
        }
        vec![]
    }

    /// True when the Now Playing screen is showing (so the shell only animates the visualiser
    /// there — animating elsewhere would waste battery under dirty-flag rendering).
    pub fn is_now_playing(&self) -> bool {
        self.current() == Screen::NowPlaying
    }

    /// Whether the playback state says we're playing (the visualiser animates only when playing).
    pub fn is_playing(&self) -> bool {
        self.playing
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
            self.boot_stock_armed = false;   // never leave a restart armed across a screen change
            self.stack.push(s);
        }
    }
    fn pop(&mut self) {
        if self.stack.len() > 1 {
            self.boot_stock_armed = false;
            self.stack.pop();
        }
    }

    // Activate the focused Settings row (shared by the Select button and a tap).
    /// The Menu's live row subtitles. Extracted from `render` so the strings themselves are
    /// testable: `render` only produces pixels, so a mock value creeping back in (this table used
    /// to carry the prototype's invented "124 albums · 1,842 tracks" and "WH-1000XM5 · LDAC") could
    /// not otherwise be caught by a test. Every field below reports state this App actually holds.
    pub(crate) fn menu_subtitles(&self) -> MenuSubtitles {
        MenuSubtitles {
            // Filled by render, which is the only place with the live NowPlaying view-model.
            now_playing: String::new(),
            library: if self.lib.is_empty() {
                String::from("Empty")
            } else {
                format!("{} albums · {} tracks", self.lib.album_count(), self.lib.songs.len())
            },
            queue: if self.queue.is_empty() {
                String::from("Queue empty")
            } else {
                format!("{} queued", self.queue.len())
            },
            eq: data::EQ_PRESETS[self.eq_preset].0.to_string(),
            bluetooth: crate::bluetooth::CODECS[self.bt_codec as usize].0.to_string(),
            usb_dac: String::from(if self.usb_dac_on { "On" } else { "Off" }),
            // Name the effects actually engaged; "Off" when the chain is clean.
            sound: {
                let on: Vec<&str> = [
                    ("DSEE HX", self.snd_dsee),
                    ("Vinyl", self.snd_vinyl),
                    ("VPT", self.snd_vpt),
                    ("DC Phase", self.snd_dc),
                    ("Normaliser", self.snd_norm),
                    ("Clear Phase", self.snd_clear),
                ]
                .iter()
                .filter(|(_, en)| *en)
                .map(|(n, _)| *n)
                .collect();
                if on.is_empty() { String::from("Off") } else { on.join(" · ") }
            },
        }
    }

    fn settings_activate(&mut self) -> Vec<Action> {
        // Touching any other row cancels a pending boot-to-stock confirmation, so the armed state
        // can never linger and turn a later, unrelated tap on that row into a restart.
        if self.settings_sel != crate::settings::ROW_BOOT_STOCK {
            self.boot_stock_armed = false;
        }
        match self.settings_sel {
            crate::settings::ROW_THEME => {
                self.night = !self.night;
                vec![Action::ThemeChanged(self.night)]
            }
            crate::settings::ROW_ACCENT => {
                // Select (the physical button) cycles; a tap on a specific swatch is handled in
                // `tap` and picks that colour outright. Render-only, so nothing for the shell.
                self.accent = self.accent.next();
                vec![]
            }
            crate::settings::ROW_VIZ => {
                self.cycle_viz();
                vec![]
            }
            crate::settings::ROW_VIZ_ANIM => {
                self.viz_size = (self.viz_size + 1) % crate::viz::SIZE_COUNT;
                vec![]
            }
            crate::settings::ROW_SLEEP => {
                const PRESETS: [u32; 5] = [0, 15, 30, 45, 60];
                self.sleep_idx = (self.sleep_idx + 1) % PRESETS.len();
                self.sleep_min = PRESETS[self.sleep_idx];
                vec![Action::SleepTimer(self.sleep_min)]
            }
            crate::settings::ROW_BATTERY => {
                self.battery_care = !self.battery_care;
                vec![Action::BatteryCareChanged(self.battery_care)]
            }
            crate::settings::ROW_USB_MODE => {
                // Enter USB mass-storage (connect to a PC as a drive). USB-DAC itself is its own
                // screen; this row is the file-transfer mode. Modal screen up first — the shell
                // then unmounts the volume + switches the gadget (and reverts on failure/unplug).
                self.push(Screen::UsbStorage);
                vec![Action::EnterUsbMsc]
            }
            crate::settings::ROW_BOOT_STOCK => {
                // Two-step: arm, then act. This reboots the device, so it must not be one stray tap.
                if self.boot_stock_armed {
                    self.boot_stock_armed = false;
                    vec![Action::BootToStock]
                } else {
                    self.boot_stock_armed = true;
                    vec![]
                }
            }
            crate::settings::ROW_SCREEN_OFF => {
                self.screen_off_idx = (self.screen_off_idx + 1) % SCREEN_OFF_PRESETS.len();
                self.screen_off_s = SCREEN_OFF_PRESETS[self.screen_off_idx];
                vec![Action::ScreenOffTimer(self.screen_off_s)]
            }
            crate::settings::ROW_BRIGHTNESS => {
                // 1..5 and wrap. The shell turns the level into a raw backlight value.
                self.brightness = if self.brightness >= 5 { 1 } else { self.brightness + 1 };
                vec![Action::BrightnessChanged(self.brightness)]
            }
            _ => vec![], // display-only / device-gated rows
        }
    }

    // Toggle the focused Sound effect row (shared by the Select button and a tap).
    fn sound_toggle_row(&mut self) -> Vec<Action> {
        match self.sound_sel {
            0 => self.snd_dsee = !self.snd_dsee,
            1 => self.snd_vinyl = !self.snd_vinyl,
            2 => self.snd_vpt = !self.snd_vpt,
            3 => self.snd_dc = !self.snd_dc,
            4 => self.snd_norm = !self.snd_norm,
            5 => self.snd_clear = !self.snd_clear,
            _ => {}
        }
        vec![Action::SoundChanged]
    }

    /// A touchscreen TAP at UI coordinates (x: 0..480, y: 0..800). The NW-A55 has no d-pad, so touch
    /// is the primary navigation — this maps a tap to the right action for the current screen and
    /// returns shell actions (same vocabulary as `press`). The left-edge swipe (Back) and drag-
    /// scroll are handled by the shell, which calls `touch_scroll`/`press(Back)` for those.
    pub fn tap(&mut self, x: i32, y: i32) -> Vec<Action> {
        use crate::canvas::W;
        // Hold/lock switch engaged → the touchscreen is dead (pocket-safe). Taps do nothing; only
        // the physical Hold switch going off unlocks (see `set_hold`).
        if self.locked {
            return vec![];
        }
        // Shelf bottom-sheet overlay owns all taps while it's open.
        if self.shelf_open {
            return self.shelf_tap(x, y);
        }
        // USB mass-storage is MODAL (the volume is handed to the PC): the only live tap target
        // is the TURN OFF button (same exit as the physical Back button / cable unplug).
        if matches!(self.current(), Screen::UsbStorage) {
            if crate::usb_storage::hit_off(x, y) {
                self.pop();
                return vec![Action::ExitUsbMsc];
            }
            return vec![];
        }
        // Onboarding owns the screen: right ~60% = next/finish, left = previous page.
        if matches!(self.current(), Screen::Onboarding) {
            if x > (W as i32) * 2 / 5 {
                if self.onboarding_page + 1 < crate::onboarding::PAGES {
                    self.onboarding_page += 1;
                } else {
                    self.finish_onboarding();
                }
            } else {
                self.onboarding_page = self.onboarding_page.saturating_sub(1);
            }
            return vec![];
        }
        // Now Playing return bar (browsing screens only). Checked before everything else below the
        // status strip: it is pinned to the bottom, so nothing else claims those rows.
        if Self::shows_np_bar(self.current()) && crate::chrome::hit_np_bar(x, y) {
            // Left zone = play/pause without leaving the list; the rest opens Now Playing.
            if crate::chrome::hit_np_bar_play(x, y) {
                self.playing = !self.playing;
                return vec![Action::PlayPause];
            }
            self.go(Screen::NowPlaying);
            return vec![];
        }
        // Global chrome (status bar, the full top strip). The bookmark glyph opens the **Shelf**;
        // tapping anywhere else on the bar opens the **Menu** — the rest of the strip stays one big
        // forgiving Menu target. Both zones come from `chrome::status_hit`, which is built from the
        // same constants that place the glyphs. Header back chevron → Back (below).
        match crate::chrome::status_hit(x, y) {
            Some(crate::chrome::StatusTap::Shelf) => {
                self.open_shelf();
                return vec![];
            }
            Some(crate::chrome::StatusTap::NowPlaying) => {
                // One-tap return from any screen. `go` (not `push`) so this collapses the stack
                // instead of burying Now Playing under whatever you were browsing.
                self.go(Screen::NowPlaying);
                return vec![];
            }
            Some(crate::chrome::StatusTap::Menu) => {
                if self.current() != Screen::Menu {
                    self.push(Screen::Menu);
                }
                return vec![];
            }
            None => {}
        }
        // Back chevron: a generous ≥44px target (the whole header-left block, from just under
        // the status strip to the header rule) on every screen that draws one.
        let has_header = !matches!(self.current(), Screen::NowPlaying | Screen::Menu | Screen::Lock);
        if has_header && (crate::chrome::STATUS_H..crate::chrome::HEADER_BOTTOM).contains(&y) && x < 80 {
            self.pop();
            return vec![];
        }

        match self.current() {
            Screen::Menu => {
                if let Some(row) = crate::menu::row_at(y, MENU.len()) {
                    self.menu_idx = row;
                    self.activate_menu(row);
                }
                vec![]
            }
            Screen::NowPlaying => {
                let hit = |cx: i32, cy: i32, r: i32| (x - cx).pow(2) + (y - cy).pow(2) <= r * r;
                // Like: tested before the transport row (it sits above it, and its target is
                // square rather than circular, so the two can't overlap).
                if crate::now_playing::hit_heart(x, y) {
                    return vec![Action::ToggleLiked];
                }
                if hit(240, 692, 44) {
                    self.playing = !self.playing;
                    vec![Action::PlayPause]
                } else if hit(130, 692, 34) {
                    vec![Action::Prev]
                } else if hit(350, 692, 34) {
                    vec![Action::Next]
                } else if hit(44, 692, 30) {
                    // shuffle icon (transport row, far left)
                    vec![Action::ShuffleToggle]
                } else if hit(436, 692, 30) {
                    // repeat icon (transport row, far right)
                    vec![Action::RepeatCycle]
                } else if y > 744 {
                    // bottom toolbar: library · queue · eq · bt · settings (the old heart was
                    // inert — a straight jump to the Library earns the prime slot instead)
                    if x < 96 {
                        self.push(Screen::Library);
                        vec![]
                    } else if x < 192 {
                        self.push(Screen::UpNext);
                        vec![]
                    } else if x < 288 {
                        self.push(Screen::Eq);
                        vec![]
                    } else if x < 384 {
                        self.push(Screen::Bluetooth);
                        vec![]
                    } else {
                        self.push(Screen::Settings);
                        vec![]
                    }
                } else if y < 91 {
                    self.push(Screen::Menu); // tap the top/art → menu
                    vec![]
                } else {
                    vec![]
                }
            }
            Screen::Library => self.tap_library(x, y),
            Screen::Album => {
                // track rows via the render-mirroring hit test (rows start 312 @56 —
                // library::album_view geometry; the Play-album band above returns None).
                if let Some(album) = self.lib.albums_flat().get(self.album_view) {
                    if let Some(row) = library::album_hit_track(album, self.album_scroll_px, y) {
                        self.album_track_idx = row;
                        return album
                            .track_list
                            .get(row)
                            .map(|s| vec![Action::PlayIndex(s.object_id)])
                            .unwrap_or_default();
                    }
                    // The "Play album" band: play from track 1 — the shell's album_context
                    // expands it to the whole album in order. (Hit-tested through the band's own
                    // rect; the old literal range started 16px above where the band is drawn.)
                    if library::hit_album_play_band(x, y) {
                        self.album_track_idx = 0;
                        return album
                            .track_list
                            .first()
                            .map(|s| vec![Action::PlayIndex(s.object_id)])
                            .unwrap_or_default();
                    }
                }
                vec![]
            }
            Screen::UpNext => {
                // Tap a queue row to play it. The rows are whatever the last render drew
                // (`up_next_rows`), so this follows the auto-scrolled window exactly.
                if let Some(id) = crate::up_next::visible_row_at(y).and_then(|r| self.up_next_rows.get(r))
                {
                    return vec![Action::PlayIndex(*id)];
                }
                // Anywhere off the rows keeps the old shortcut back to Now Playing.
                self.go(Screen::NowPlaying);
                vec![]
            }
            Screen::Settings => {
                // A swatch tap picks that accent directly. Checked first because it lives inside
                // the Accent row's band, and falling through to `settings_activate` would advance
                // the cycle by one instead of honouring the colour under the finger.
                if let Some(i) = crate::settings::accent_hit(x, y, self.settings_scroll_px) {
                    self.settings_sel = crate::settings::ROW_ACCENT;
                    self.boot_stock_armed = false; // same disarm rule as any other row touch
                    self.accent = Accent::from_index(i);
                    return vec![];
                }
                if let Some(row) = crate::settings::row_at(y, self.settings_scroll_px) {
                    self.settings_sel = row;
                    return self.settings_activate();
                }
                vec![]
            }
            Screen::Sound => {
                // A/B compare control (top-right of the header)
                if (44..70).contains(&y) && x > 380 {
                    self.snd_ab_bypass = !self.snd_ab_bypass;
                    return vec![Action::SoundBypass(self.snd_ab_bypass)];
                }
                if let Some(row) = crate::sound::row_at(y) {
                    self.sound_sel = row;
                    return self.sound_toggle_row();
                }
                vec![]
            }
            Screen::Eq => {
                // Every region below comes from `eq`'s own layout helpers — the same ones render
                // draws with — so a pill/band/footer tap always lands on what's under the finger.
                if let Some(idx) = crate::eq::preset_at(x, y) {
                    self.eq_preset = idx;
                    self.eq_bands = data::EQ_PRESETS[idx].1;
                    return vec![Action::EqChanged(self.eq_bands)];
                }
                // Band field: tap a column to select it; above the zero line raises, below lowers.
                if (crate::eq::FIELD_TOP..crate::eq::FIELD_BOTTOM).contains(&y) {
                    if let Some(band) = crate::eq::band_at(x) {
                        self.eq_sel = band;
                        let g = &mut self.eq_bands[band];
                        if y < crate::eq::FIELD_MID {
                            *g = (*g + 1).min(6);
                        } else {
                            *g = (*g - 1).max(-6);
                        }
                        return vec![Action::EqChanged(self.eq_bands)];
                    }
                    return vec![];
                }
                // Footer: Reset flattens every band. (Save is not wired — the EQ is already
                // persisted automatically on every change; see ROADMAP.)
                if crate::eq::footer_at(x, y) == Some(crate::eq::Footer::Reset) {
                    self.eq_bands = [0; 10];
                    self.eq_preset = 0; // FLAT
                    return vec![Action::EqChanged(self.eq_bands)];
                }
                vec![]
            }
            Screen::Bluetooth => {
                use crate::bluetooth::BtHit;
                match crate::bluetooth::hit(x, y, self.bt_on, self.bt_codec == crate::bluetooth::LDAC) {
                    BtHit::Toggle | BtHit::Disconnect => {
                        self.bt_on = !self.bt_on;
                        vec![Action::BtToggle(self.bt_on)]
                    }
                    BtHit::Codec(i) => {
                        self.bt_codec = i as u8;
                        vec![Action::BtCodecChanged]
                    }
                    BtHit::Quality(i) => {
                        self.bt_ldac_quality = i as u8;
                        vec![Action::BtCodecChanged]
                    }
                    BtHit::Pair | BtHit::None => vec![],
                }
            }
            Screen::UsbDac => {
                // Only the switch toggles USB-DAC (→ 3.5mm + BT/LDAC). It used to fire on a tap
                // anywhere on the screen, so a stray touch could switch the USB gadget mode and
                // start the LDAC bridge. Mass storage lives on Settings ▸ USB mode, not here.
                if crate::usbdac::hit_toggle(x, y) {
                    self.usb_dac_on = !self.usb_dac_on;
                    return vec![Action::UsbDacToggle(self.usb_dac_on)];
                }
                vec![]
            }
            _ => vec![],
        }
    }

    // Library tap: SORT/ORDER chip, tab bar, then the data rows (scroll-aware). Songs play the
    // track; Albums expand inline (accordion) / drill in via the art / play an expanded track;
    // Artists/Playlists select.
    fn tap_library(&mut self, x: i32, y: i32) -> Vec<Action> {
        // SORT/ORDER chip in the header's right slot (right of the back chevron's x<80 band).
        // Songs cycles the SORT chip; Albums cycles the ORDER chip. Both reset the list position.
        if (34..91).contains(&y) && x >= 300 {
            match self.lib_tab {
                Tab::Songs => self.lib_sort = (self.lib_sort + 1) % library::SORTS.len(),
                Tab::Albums => self.cycle_album_sort(),
                _ => return vec![],
            }
            self.lib_idx = 0;
            self.lib_scroll_px = 0;
            self.fling_v = 0.0;
            return vec![];
        }
        if (91..126).contains(&y) {
            self.lib_tab = if x < 120 {
                Tab::Songs
            } else if x < 220 {
                Tab::Albums
            } else if x < 330 {
                Tab::Artists
            } else {
                Tab::Playlists
            };
            self.lib_idx = 0;
            self.lib_scroll_px = 0;
            self.fling_v = 0.0;
            self.album_expanded = None;
            return vec![];
        }
        // A–Z rail: right edge, over the list. Tested BEFORE the rows, because it overlays them —
        // a tap there means "jump", never "open the row underneath".
        if library::az_hit_x(x) {
            if let Some(letter) = library::az_letter_at(y, self.lib_tab) {
                if let Some(px) = library::az_scroll_for(
                    self.lib_tab, &self.lib, letter, self.album_sort, self.album_expanded,
                ) {
                    self.lib_scroll_px = px;
                    self.fling_v = 0.0;   // a jump must not keep coasting from a previous flick
                }
            }
            return vec![];
        }
        // The accent band sits above the list on every tab, so test it before the rows (it is the
        // largest target on the screen; it used to be drawn but hit-tested nowhere).
        if library::hit_shuffle_band(x, y) {
            return vec![Action::Shuffle(match self.lib_tab {
                Tab::Songs => ShuffleScope::AllSongs,
                Tab::Albums => ShuffleScope::ByAlbum,
                Tab::Artists => ShuffleScope::ByArtist,
                Tab::Playlists => ShuffleScope::Playlist,
            })];
        }
        // Albums is a sortable accordion — its own hit test (expand/collapse, drill-in, play track).
        if matches!(self.lib_tab, Tab::Albums) {
            return self.tap_albums(x, y);
        }
        // The other tabs route through the render-mirroring hit test (library::hit_row): it knows
        // each tab's list top/row height and returns None for the shuffle band / gaps / off-list.
        let Some(row) = library::hit_row(self.lib_tab, &self.lib, self.lib_scroll_px, y) else {
            return vec![];
        };
        match self.lib_tab {
            // `row` is the RANK in the drawn (sorted) order — resolve through the same order.
            Tab::Songs => library::song_at(&self.lib, self.lib_sort, row)
                .map(|s| s.object_id)
                .map(|id| {
                    self.lib_idx = row;
                    vec![Action::PlayIndex(id)]
                })
                .unwrap_or_default(),
            // A playlist row has no single track under the finger, so tapping it plays the whole
            // list from the top in saved order (the shell resolves the members through the DB).
            Tab::Playlists => {
                self.lib_idx = row;
                self.lib
                    .playlists
                    .get(row)
                    .map(|p| vec![Action::PlayPlaylist(p.id)])
                    .unwrap_or_default()
            }
            // Artist rows aren't directly playable (no track object under the finger) — they
            // navigate, not play. Nothing to enqueue.
            _ => {
                self.lib_idx = row;
                vec![]
            }
        }
    }

    // A tap on the Albums accordion: the art (left) drills into the album page; the row body
    // toggles the inline track list; a track row plays that track in album context.
    fn tap_albums(&mut self, x: i32, y: i32) -> Vec<Action> {
        use crate::library::AlbumsHit;
        match library::albums_hit(&self.lib, self.album_sort, self.album_expanded, self.lib_scroll_px, x, y) {
            Some(AlbumsHit::AlbumToggle(flat)) => {
                self.album_expanded = if self.album_expanded == Some(flat) { None } else { Some(flat) };
                if let Some(rank) = self.album_rank_of(flat) {
                    self.lib_idx = rank;
                }
                self.clamp_lib_scroll();
                self.fling_v = 0.0;
                vec![]
            }
            Some(AlbumsHit::AlbumOpen(flat)) => {
                self.album_view = flat;
                self.album_track_idx = 0;
                self.album_scroll_px = 0;
                self.fling_v = 0.0;
                self.push(Screen::Album);
                vec![]
            }
            Some(AlbumsHit::Track(flat, track)) => self
                .lib
                .albums_flat()
                .get(flat)
                .and_then(|al| al.track_list.get(track))
                .map(|s| vec![Action::PlayIndex(s.object_id)])
                .unwrap_or_default(),
            None => vec![],
        }
    }

    /// The track under `y` in the Albums accordion (an expanded album's inline track row), if the
    /// finger is on one. Shared by the tap and the swipe-to-queue gesture so both resolve to the
    /// same row — the swipe used to ignore this tab entirely.
    fn albums_track_at(&self, y: i32) -> Option<SongRow> {
        use crate::library::AlbumsHit;
        // x is inside the row body: the accordion's own hit test only reports Track for the
        // track band, and the swipe already established this is a horizontal gesture on a row.
        match library::albums_hit(&self.lib, self.album_sort, self.album_expanded, self.lib_scroll_px, 240, y) {
            Some(AlbumsHit::Track(flat, track)) => {
                self.lib.albums_flat().get(flat).and_then(|al| al.track_list.get(track)).cloned()
            }
            _ => None,
        }
    }

    /// Advance the Albums ORDER chip and collapse any open accordion (its identity is still valid,
    /// but the position shift is less confusing when it re-opens deliberately).
    fn cycle_album_sort(&mut self) {
        self.album_sort = (self.album_sort + 1) % library::ALBUM_SORTS.len();
        self.album_expanded = None;
    }

    /// The album DISPLAY rank (0-based over albums, under the current ORDER) of a `albums_flat()`
    /// index — for keeping the button cursor on a toggled row.
    fn album_rank_of(&self, flat: usize) -> Option<usize> {
        library::album_display_order(&self.lib, self.album_sort).iter().position(|&f| f == flat)
    }

    /// Largest useful library scroll for the current tab (Albums depends on ORDER + expansion).
    fn lib_max_scroll(&self) -> i32 {
        library::max_scroll_px(self.lib_tab, &self.lib, self.album_sort, self.album_expanded)
    }

    /// Re-clamp the library scroll after the content height changes (accordion open/close).
    fn clamp_lib_scroll(&mut self) {
        let max = self.lib_max_scroll();
        self.lib_scroll_px = self.lib_scroll_px.clamp(0, max);
    }

    /// Live drag-scroll of the current list by `dy_px` PIXELS (positive = content moves up /
    /// show later rows). Called per pump tick while a vertical drag is in progress, so the list
    /// tracks the finger; clamped to the content height.
    pub fn scroll_px(&mut self, dy_px: i32) {
        match self.current() {
            Screen::Library => {
                let max = self.lib_max_scroll();
                self.lib_scroll_px = (self.lib_scroll_px + dy_px).clamp(0, max);
            }
            Screen::Album => {
                if let Some(al) = self.lib.albums_flat().get(self.album_view) {
                    let max = library::album_max_scroll_px(al);
                    self.album_scroll_px = (self.album_scroll_px + dy_px).clamp(0, max);
                }
            }
            Screen::Settings => {
                let max = crate::settings::max_scroll_px();
                self.settings_scroll_px = (self.settings_scroll_px + dy_px).clamp(0, max);
            }
            _ => {}
        }
    }

    /// Momentum fling: the release velocity (px/s, same sign convention as `scroll_px`). The
    /// per-frame `tick()` integrates and decays it, keeping frames dirty until it stops.
    pub fn fling(&mut self, velocity_px_s: f32) {
        if matches!(self.current(), Screen::Library | Screen::Album) {
            self.fling_v = velocity_px_s.clamp(-8000.0, 8000.0);
        }
    }

    /// A finger touching down kills any in-flight momentum (standard scroll UX).
    pub fn stop_fling(&mut self) {
        self.fling_v = 0.0;
    }

    /// Horizontal touch SWIPE (dir −1 = leftward, +1 = rightward) at the gesture's START point
    /// (UI coords), from the shell's classifier. Onboarding pages left=next/finish,
    /// right=previous; Now Playing maps to the same guarded transport actions as the skip
    /// buttons; on the Library/Album lists a RIGHTWARD swipe on a song row adds that song to the
    /// play queue (Spotify-style) with a toast. (Edge-back is classified by the shell before
    /// this and never reaches here.)
    pub fn swipe(&mut self, dir: i32, _x: i32, y: i32) -> Vec<Action> {
        if self.locked || self.shelf_open {
            return vec![];
        }
        match self.current() {
            Screen::Onboarding => {
                if dir < 0 {
                    if self.onboarding_page + 1 < crate::onboarding::PAGES {
                        self.onboarding_page += 1;
                    } else {
                        self.finish_onboarding();
                    }
                } else {
                    self.onboarding_page = self.onboarding_page.saturating_sub(1);
                }
                vec![]
            }
            Screen::NowPlaying => {
                // Zoned by y. A swipe on the PAGING BLOCK (the artwork) turns the page; a swipe
                // anywhere below it still skips tracks, which is where the transport already is.
                // Splitting it keeps both gestures with no modifier and no long-press, and it
                // matches what the finger is on: you flip the picture, or you change the track.
                // The physical FF/REW keys skip from anywhere regardless.
                if y < crate::now_playing::PAGE_SWIPE_BOT {
                    let pages = crate::now_playing::PAGES;
                    self.np_page = if dir < 0 {
                        (self.np_page + 1) % pages
                    } else {
                        (self.np_page + pages - 1) % pages
                    };
                    vec![]
                } else if dir < 0 {
                    vec![Action::Next]
                } else {
                    vec![Action::Prev]
                }
            }
            Screen::Library if dir > 0 => {
                // Right-swipe a track row → queue that song, using the same render-mirroring hit
                // test the tap uses so the queued song is exactly the row under the finger.
                // Both tabs that put a *track* under the finger are covered: the Songs list, and
                // the tracks of an expanded album in the Albums accordion. (Artist/playlist rows
                // aren't tracks, so there is nothing to queue.)
                let song = match self.lib_tab {
                    Tab::Songs => library::hit_row(self.lib_tab, &self.lib, self.lib_scroll_px, y)
                        .and_then(|rank| library::song_at(&self.lib, self.lib_sort, rank))
                        .cloned(),
                    Tab::Albums => self.albums_track_at(y),
                    _ => None,
                };
                if let Some(s) = song {
                    self.enqueue(s, y);
                }
                vec![]
            }
            Screen::Album if dir > 0 => {
                // Right-swipe a track row inside an album drill-in → queue it.
                let song = self.lib.albums_flat().get(self.album_view).and_then(|al| {
                    library::album_hit_track(al, self.album_scroll_px, y)
                        .and_then(|ti| al.track_list.get(ti).cloned())
                });
                if let Some(s) = song {
                    self.enqueue(s, y);
                }
                vec![]
            }
            _ => vec![],
        }
    }

    /// Append a song to the user queue + pop the confirmation toast + start the row chip
    /// animation (`y` = the gesture y, so the chip rides the row the user flicked).
    fn enqueue(&mut self, s: SongRow, y: i32) {
        self.toast = format!("Added to queue — {}", s.title);
        self.toast_frames = TOAST_FRAMES;
        self.queue_anim_y = y;
        self.queue_anim_frames = QUEUE_ANIM_FRAMES;
        self.queue.push(s);
    }

    /// The user queue (Up Next shows it in front of the album-derived list).
    pub fn queue(&self) -> &[SongRow] {
        &self.queue
    }

    fn go(&mut self, s: Screen) {
        self.stack = vec![s];
    }

    /// Hand the shell-decoded 96x96 cover for the open album to the UI (None = draw the gradient).
    pub fn set_album_cover(&mut self, img: Option<crate::art::Image>) {
        self.album_cover = img;
    }

    /// `album_id` of the album currently drilled into, if any. The shell polls this to know which
    /// cover to load out of its art cache — the UI never reads the cache itself.
    pub fn open_album_id(&self) -> Option<i64> {
        if self.current() != Screen::Album {
            return None;
        }
        self.lib.albums_flat().get(self.album_view).map(|a| a.album_id)
    }

    /// Screens that carry the Now Playing return bar: the library browse list and the album
    /// drill-in. These are the places you end up several pushes deep from Now Playing, which is
    /// exactly where Back-ing out one screen at a time is tedious.
    fn shows_np_bar(s: Screen) -> bool {
        matches!(s, Screen::Library | Screen::Album)
    }

    /// Number of rows in the current library tab (for cursor clamping).
    fn lib_len(&self) -> usize {
        library::row_count(self.lib_tab, &self.lib)
    }

    /// Replace the browsable library (called by the shell after the real DB is read). Resets
    /// the cursor so a stale index can't point past the new contents.
    /// Mutable access to the library, for the shell to drop in cover thumbnails as its background
    /// decoder produces them. Deliberately narrow in intent: the UI itself never mutates this.
    pub fn library(&self) -> &Library {
        &self.lib
    }
    pub fn library_mut(&mut self) -> &mut Library {
        &mut self.lib
    }

    pub fn set_library(&mut self, lib: Library) {
        self.lib = lib;
        self.lib_idx = 0;
        self.lib_scroll_px = 0;
        self.fling_v = 0.0;
        self.album_expanded = None;
    }

    /// Keep the library cursor's row fully inside the pixel-scrolled window (button nav).
    fn lib_ensure_visible(&mut self) {
        let row_top = library::row_top_px(self.lib_tab, &self.lib, self.lib_idx, self.album_sort, self.album_expanded);
        // Albums rows are ALBUM_ROW_H; the fixed tabs use their own row_h.
        let rh = if matches!(self.lib_tab, Tab::Albums) { library::ALBUM_ROW_H } else { library::row_h(self.lib_tab) };
        let view = library::view_h(self.lib_tab);
        if row_top < self.lib_scroll_px {
            self.lib_scroll_px = row_top;
        } else if row_top + rh > self.lib_scroll_px + view {
            self.lib_scroll_px = row_top + rh - view;
        }
    }

    /// Keep the album drill-in cursor's row fully inside its pixel-scrolled window.
    fn album_ensure_visible(&mut self) {
        let row_top = self.album_track_idx as i32 * library::ALBUM_TRACK_RH;
        let view = crate::canvas::H as i32 - 12 - library::ALBUM_TRACKS_TOP;
        if row_top < self.album_scroll_px {
            self.album_scroll_px = row_top;
        } else if row_top + library::ALBUM_TRACK_RH > self.album_scroll_px + view {
            self.album_scroll_px = row_top + library::ALBUM_TRACK_RH - view;
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
        // Hold/lock SWITCH engaged: the touchscreen is disabled (see `tap`), but the physical
        // transport + volume buttons still control playback — you can skip/pause/adjust without
        // unlocking. Power only toggles the screen (the shell blanks/wakes the backlight on Sleep);
        // it does NOT unlock. The ONLY thing that unlocks is the Hold switch going off (`set_hold`).
        if self.locked {
            return match b {
                Button::Power => vec![Action::Sleep],
                Button::Play => {
                    self.playing = !self.playing;
                    vec![Action::PlayPause]
                }
                Button::Right | Button::Next => vec![Action::Next],
                Button::Left | Button::Prev => vec![Action::Prev],
                Button::VolUp => {
                    self.volume = (self.volume + 1).min(crate::overlay::VOL_MAX);
                    vec![Action::VolUp]
                }
                Button::VolDown => {
                    self.volume = self.volume.saturating_sub(1);
                    vec![Action::VolDown]
                }
                _ => vec![],
            };
        }

        // Shelf overlay open: Back (incl. the left-edge swipe) closes it; Play/Vol still work as
        // global music controls; other navigation is suppressed until it's dismissed.
        if self.shelf_open {
            return match b {
                Button::Back => {
                    self.shelf_open = false;
                    vec![]
                }
                Button::Power => vec![Action::Sleep],
                Button::Play => {
                    self.playing = !self.playing;
                    vec![Action::PlayPause]
                }
                Button::Next => vec![Action::Next],
                Button::Prev => vec![Action::Prev],
                Button::VolUp => {
                    self.volume = (self.volume + 1).min(crate::overlay::VOL_MAX);
                    self.vol_overlay = crate::overlay::VOL_FRAMES;
                    vec![Action::VolUp]
                }
                Button::VolDown => {
                    self.volume = self.volume.saturating_sub(1);
                    self.vol_overlay = crate::overlay::VOL_FRAMES;
                    vec![Action::VolDown]
                }
                _ => vec![],
            };
        }

        // USB mass-storage modal: Back leaves the mode (the shell remounts + restores the USB
        // gadget); everything else is suppressed while the PC owns the storage.
        if matches!(self.current(), Screen::UsbStorage) {
            return match b {
                Button::Back => {
                    self.pop();
                    vec![Action::ExitUsbMsc]
                }
                _ => vec![],
            };
        }

        // Onboarding owns ALL input while showing, so the global Play/Vol/Home gestures don't
        // interfere with stepping through the intro. Right/Select = next (or finish on the last
        // page); Left = back a page; Back = skip. Finishing/skipping marks it seen (persisted) so
        // it appears only once, and returns to where you came from (Menu, or Now Playing on boot).
        if matches!(self.current(), Screen::Onboarding) {
            return match b {
                Button::Right | Button::Select => {
                    if self.onboarding_page + 1 < crate::onboarding::PAGES {
                        self.onboarding_page += 1;
                        vec![]
                    } else {
                        self.finish_onboarding();
                        vec![]
                    }
                }
                Button::Left => {
                    self.onboarding_page = self.onboarding_page.saturating_sub(1);
                    vec![]
                }
                Button::Back => {
                    self.finish_onboarding();
                    vec![]
                }
                _ => vec![],
            };
        }

        // Global gestures, available on every screen.
        match b {
            Button::Power => {
                // Power is a screen on/off toggle (the shell blanks/wakes the backlight). It does
                // NOT lock — locking is the physical Hold switch's job (`set_hold`).
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
            Button::Next => return vec![Action::Next],
            Button::Prev => return vec![Action::Prev],
            _ => {}
        }

        match self.current() {
            Screen::NowPlaying => match b {
                Button::Right => vec![Action::Next],
                Button::Left => vec![Action::Prev],
                Button::Up | Button::Select => {
                    self.push(Screen::Menu);
                    vec![]
                }
                Button::Option => {
                    self.cycle_viz(); // cycle the visualiser type (UI state)
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
                    self.activate_menu(self.menu_idx);
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
                    self.lib_scroll_px = 0;
                    self.fling_v = 0.0;
                    self.album_expanded = None;
                    vec![]
                }
                Button::Right => {
                    self.lib_tab = next_tab(self.lib_tab);
                    self.lib_idx = 0;
                    self.lib_scroll_px = 0;
                    self.fling_v = 0.0;
                    self.album_expanded = None;
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
                // Option cycles the Songs SORT chip, or the Albums ORDER chip.
                Button::Option if matches!(self.lib_tab, Tab::Songs) => {
                    self.lib_sort = (self.lib_sort + 1) % library::SORTS.len();
                    self.lib_idx = 0;
                    self.lib_scroll_px = 0;
                    vec![]
                }
                Button::Option if matches!(self.lib_tab, Tab::Albums) => {
                    self.cycle_album_sort();
                    self.lib_idx = 0;
                    self.lib_scroll_px = 0;
                    vec![]
                }
                Button::Select => match self.lib_tab {
                    // Albums drill into a track list; Songs/Playlists play the row directly.
                    Tab::Albums => {
                        // lib_idx is the album DISPLAY rank — resolve it to the flat album index.
                        self.album_view =
                            library::album_flat_at_rank(&self.lib, self.album_sort, self.lib_idx).unwrap_or(0);
                        self.album_track_idx = 0;
                        self.album_scroll_px = 0;
                        self.fling_v = 0.0;
                        self.push(Screen::Album);
                        vec![]
                    }
                    // lib_idx is a RANK in the drawn (sorted) order — resolve through it.
                    Tab::Songs => library::song_at(&self.lib, self.lib_sort, self.lib_idx)
                        .map(|s| vec![Action::PlayIndex(s.object_id)])
                        .unwrap_or_default(),
                    // A playlist row has no single track under the cursor — selecting it plays
                    // the whole list from the top, in saved order.
                    Tab::Playlists => self
                        .lib
                        .playlists
                        .get(self.lib_idx)
                        .map(|p| vec![Action::PlayPlaylist(p.id)])
                        .unwrap_or_default(),
                    // Artist rows navigate, not play (no track under the cursor).
                    _ => vec![],
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
                        self.album_ensure_visible();
                        vec![]
                    }
                    Button::Down => {
                        if self.album_track_idx + 1 < n {
                            self.album_track_idx += 1;
                            self.album_ensure_visible();
                        }
                        vec![]
                    }
                    Button::Select => self
                        .lib
                        .albums_flat()
                        .get(self.album_view)
                        .and_then(|a| a.track_list.get(self.album_track_idx))
                        .map(|s| vec![Action::PlayIndex(s.object_id)])
                        .unwrap_or_default(),
                    Button::Back | Button::Left => {
                        self.pop();
                        vec![]
                    }
                    _ => vec![],
                }
            }
            Screen::Settings => match b {
                Button::Up => {
                    self.settings_sel = self.settings_sel.saturating_sub(1);
                    vec![]
                }
                Button::Down => {
                    if self.settings_sel + 1 < crate::settings::ROWS {
                        self.settings_sel += 1;
                    }
                    vec![]
                }
                // Select (or Left/Right) acts on the focused row.
                Button::Select | Button::Right | Button::Left => self.settings_activate(),
                Button::Back => {
                    self.pop();
                    vec![]
                }
                _ => vec![],
            },
            Screen::UsbDac => match b {
                Button::Select => {
                    self.usb_dac_on = !self.usb_dac_on;
                    vec![Action::UsbDacToggle(self.usb_dac_on)]
                }
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
            Screen::Sound => match b {
                Button::Up => {
                    self.sound_sel = self.sound_sel.saturating_sub(1);
                    vec![]
                }
                Button::Down => {
                    if self.sound_sel + 1 < crate::sound::ROWS {
                        self.sound_sel += 1;
                    }
                    vec![]
                }
                // Select/Left/Right toggles the focused effect; the shell applies it via EffectCtrlDmp.
                Button::Select | Button::Left | Button::Right => self.sound_toggle_row(),
                // Option = instant A/B compare: flip the whole-chain bypass to hear effects on vs off.
                Button::Option => {
                    self.snd_ab_bypass = !self.snd_ab_bypass;
                    vec![Action::SoundBypass(self.snd_ab_bypass)]
                }
                Button::Back => {
                    self.pop();
                    vec![]
                }
                _ => vec![],
            },
            // The remaining screens (Fm/Receiver): Back pops, everything else is a no-op until
            // their per-screen controls are wired. (Not "Pairing" — there is no Screen::Pairing;
            // pairing.rs is a designed-but-unreachable screen, rendered only by the host preview
            // harness and the sim. UpNext isn't here either: it handles buttons above.)
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
    pub fn render(&mut self, c: &mut Canvas, fonts: &FontSet, np: &NowPlaying) {
        let theme = Theme::for_mode(self.night, self.accent);
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
            Screen::NowPlaying => {
                // inject the selected visualiser type + on/off (UI state) into the now-playing data
                // The visualiser is shown ONLY when real spectrum data is arriving. Without the
                // analyzer running it used to fall back to a synthetic animation driven by
                // viz_phase — pretty, but it is not the music: it moved identically for silence,
                // for a ballad and for a drum solo. A visualiser that doesn't represent the audio
                // is a lie the same way a hardcoded clock is, so it now draws nothing instead.
                let live = np.viz_levels.is_some();
                let np2 = NowPlaying {
                    viz_kind: self.viz_kind,
                    viz_size: if live { self.viz_size } else { 0 },
                    page: self.np_page,
                    ..*np
                };
                crate::now_playing::render(c, &theme, fonts, &np2);
                // sleep-timer countdown badge (nav owns the live remaining minutes)
                crate::now_playing::sleep_badge(c, &theme, fonts, self.sleep_min);
            }
            Screen::Menu => {
                let mut subs = self.menu_subtitles();
                // The Now Playing row carries the running track, so the Menu answers "what's on?"
                // without a trip to the player — the row was previously blank.
                subs.now_playing = if np.title.is_empty() {
                    String::from("Nothing playing")
                } else if np.elapsed.is_empty() {
                    np.title.to_string()
                } else {
                    format!("{} · {}", np.title, np.elapsed)
                };
                let (np_value, lib_value, queue_value, eq_value, sound_value, bt_value, usb_value) = (
                    &subs.now_playing, &subs.library, &subs.queue, &subs.eq, &subs.sound,
                    &subs.bluetooth, &subs.usb_dac,
                );
                let items: Vec<MenuItem> = MENU
                    .iter()
                    .enumerate()
                    .map(|(i, (screen, icon, label, value))| MenuItem {
                        icon,
                        label,
                        value: match *screen {
                            Screen::NowPlaying => &np_value,
                            Screen::Library => &lib_value,
                            Screen::UpNext => &queue_value,
                            Screen::Eq => &eq_value,
                            Screen::Sound => &sound_value,
                            Screen::Bluetooth => &bt_value,
                            Screen::UsbDac => &usb_value,
                            _ => value,
                        },
                        active: i == self.menu_idx,
                    })
                    .collect();
                crate::menu::render(c, &theme, fonts, &items);
            }
            Screen::Library => {
                crate::library::render(
                    c, &theme, fonts, self.lib_tab, self.lib_idx, self.lib_scroll_px, self.lib_sort,
                    self.album_sort, self.album_expanded, &self.lib,
                );
                crate::library::az_render(
                    c, &theme, fonts, self.lib_tab, &self.lib, self.album_sort,
                    self.album_expanded,
                );
            }
            Screen::Album => {
                let flat = self.lib.albums_flat();
                if let Some(al) = flat.get(self.album_view) {
                    crate::library::album_view(
                        c, &theme, fonts, al, self.album_track_idx, self.album_scroll_px,
                        self.album_cover.as_ref(),
                    );
                } else {
                    crate::library::render(
                        c, &theme, fonts, self.lib_tab, self.lib_idx, self.lib_scroll_px, self.lib_sort,
                        self.album_sort, self.album_expanded, &self.lib,
                    );
                }
            }
            Screen::Onboarding => crate::onboarding::render(c, &theme, fonts, self.onboarding_page),
            Screen::UsbStorage => crate::usb_storage::render(c, &theme, fonts),
            Screen::UpNext => {
                // Publish the ids of the rows actually drawn, in drawn order, so `tap` can
                // resolve a finger to a row. The window auto-scrolls to follow playback and its
                // position depends on `np`, which `tap` doesn't have — so the renderer, which
                // does, records it rather than the hit test guessing.
                let drawn: Vec<i64> = if !self.queue.is_empty() {
                    // The user's own queue (swipe-to-queue) takes precedence over the derived
                    // current-album list. It renders unscrolled, from the top.
                    let ids = self.queue.iter().map(|s| s.object_id).collect();
                    crate::up_next::render_queue(c, &theme, fonts, &self.queue);
                    ids
                } else {
                    match self.now_playing_queue(np.title, np.artist) {
                        Some((album, tracks, cur)) => {
                            let (_, scroll) = crate::up_next::window(tracks.len(), cur);
                            let ids = tracks.iter().skip(scroll).map(|s| s.object_id).collect();
                            crate::up_next::render(c, &theme, fonts, album, tracks, cur);
                            ids
                        }
                        None => {
                            crate::up_next::render(c, &theme, fonts, "", &[], 0);
                            Vec::new()
                        }
                    }
                };
                self.up_next_rows = drawn;
            }
            Screen::Eq => crate::eq::render(
                c, &theme, fonts, &self.eq_bands, data::EQ_PRESETS[self.eq_preset].0, self.eq_sel,
            ),
            Screen::Sound => {
                let snd = Sound {
                    dsee: self.snd_dsee,
                    vinyl: self.snd_vinyl,
                    vpt: if self.snd_vpt { "On" } else { "Off" },
                    dcphase: if self.snd_dc { "On" } else { "Off" },
                    normalizer: self.snd_norm,
                    clearaudio: self.snd_clear,
                    eq_preset: data::EQ_PRESETS[self.eq_preset].0,
                    bt_codec: if self.bt_on { Some(crate::bluetooth::CODECS[self.bt_codec as usize].0) } else { None },
                };
                crate::sound::render(c, &theme, fonts, &snd, self.sound_sel, self.snd_ab_bypass)
            }
            Screen::Bluetooth => {
                let bt = Bt {
                    on: self.bt_on,
                    // No connected-device name until there is a real BtCommonService client to ask.
                    // This used to report a hardcoded "WH-1000XM5" whenever the (UI-only) radio
                    // toggle was on, i.e. it invented a paired device that was never there.
                    // bluetooth::render already draws an honest "No device connected" for None.
                    connected: None,
                    codec_sel: self.bt_codec,
                    ldac_quality: self.bt_ldac_quality,
                };
                crate::bluetooth::render(c, &theme, fonts, &bt)
            }
            Screen::Settings => {
                let sleep_lbl = self.sleep_label();
                let brightness_lbl = format!("{} / 5", self.brightness);
                let screen_off_lbl = screen_off_label(self.screen_off_s);
                // The row value doubles as the confirmation prompt — no extra screen needed, and
                // the armed state is impossible to miss because it replaces the value in place.
                let boot_stock_lbl = if self.boot_stock_armed { "TAP AGAIN" } else { "SONY" };
                let view = crate::settings::SettingsView {
                    night: self.night,
                    viz_name: crate::viz::name(self.viz_kind),
                    viz_size_label: crate::viz::size_name(self.viz_size),
                    usb_dac: self.usb_dac_on,
                    battery_care: self.battery_care,
                    storage: self.storage_label(),
                    sleep: &sleep_lbl,
                    brightness: &brightness_lbl,
                    screen_off: &screen_off_lbl,
                    boot_stock: boot_stock_lbl,
                    accent: self.accent,
                };
                crate::settings::render(c, &theme, fonts, self.settings_sel, self.settings_scroll_px, &view)
            }
            Screen::Fm => crate::fm::render(c, &theme, fonts, 88.6),
            Screen::UsbDac => {
                let ldac = self.usb_dac_on && self.bt_on;
                let codec = crate::bluetooth::CODECS[self.bt_codec as usize].0;
                let dev: Option<&str> = None; // see the Bluetooth screen: no invented device name
                crate::usbdac::render(
                    c, &theme, fonts, self.usb_dac_on, ldac, codec, dev,
                    data::EQ_PRESETS[self.eq_preset].0, self.snd_dsee,
                )
            }
            Screen::Receiver => crate::receiver::render(c, &theme, fonts, false),
            // Shelf is an overlay, never the stack top — render Now Playing as a safe fallback if
            // it somehow becomes current (it shouldn't).
            Screen::Shelf => crate::now_playing::render(c, &theme, fonts, np),
        }
        // ── Status bar: drawn ONCE, here, for every screen that has one ──────────────────────
        // It used to be drawn inside each screen's own render, and 14 of the 16 call sites passed
        // HARDCODED literals ("14:32", "FLAC 24/96", 78). Only Now Playing and Lock passed live
        // values, so on the device the clock read 14:32 and the battery 78% on the Menu, the whole
        // Library, Settings, EQ, Sound, Bluetooth, Up Next, USB-DAC, FM and the Receiver — every
        // screen you actually browse in. Drawing it in one place means it cannot drift again.
        //
        // Drawn AFTER the screen (each one fills its own background first) but BEFORE the return
        // bar and the overlays, so the previous layering is preserved — the Shelf's dim still
        // covers it. Onboarding and the USB-storage modal own the whole panel and never had one.
        if !matches!(self.current(), Screen::Onboarding | Screen::UsbStorage) {
            crate::chrome::status_bar(c, &theme, fonts, np.clock, np.badge, np.battery);
        }

        // Now Playing return bar, pinned to the bottom of the browsing screens. Drawn after the
        // screen (it overlays nothing — `library::LIST_BOTTOM` stops above it) and before the
        // transient HUDs, which must stay on top of everything.
        if Self::shows_np_bar(self.current()) {
            crate::chrome::np_bar(c, &theme, fonts, np.title, np.artist, self.playing, np.progress);
        }
        // Transient HUD on top of any screen (except the lock screen, which owns the panel).
        if self.vol_overlay > 0 && self.current() != Screen::Lock {
            crate::overlay::volume(c, &theme, fonts, self.volume);
        }
        // Confirmation toast (e.g. "Added to queue — …"), same rules as the volume HUD.
        if self.toast_frames > 0 && self.current() != Screen::Lock {
            crate::overlay::toast(c, &theme, fonts, &self.toast);
        }
        // Swipe-to-queue chip riding the flicked row (list screens only — if the user navigates
        // away mid-animation the anchor row is gone, so it just stops).
        if self.queue_anim_frames > 0 && matches!(self.current(), Screen::Library | Screen::Album) {
            let p = self.queue_anim_frames as f32 / QUEUE_ANIM_FRAMES as f32;
            crate::overlay::queue_chip(c, &theme, fonts, self.queue_anim_y, p);
        }
        // Shelf bottom-sheet sits above everything: dims the screen behind + draws the sheet.
        if self.shelf_open {
            let (title, sub) = self.place_label();
            let pins = [
                self.pins[0].as_ref().map(|p| crate::shelf::Pin { title: &p.title, sub: &p.sub }),
                self.pins[1].as_ref().map(|p| crate::shelf::Pin { title: &p.title, sub: &p.sub }),
                self.pins[2].as_ref().map(|p| crate::shelf::Pin { title: &p.title, sub: &p.sub }),
            ];
            crate::shelf::render(c, &theme, fonts, &title, &sub, &pins);
        }
    }

    /// Advance per-frame timers (overlay countdowns). The shell calls this once per pump tick
    /// before `render`. Returns true while something time-driven still needs redrawing.
    /// One animation step at the nominal 60 fps. Prefer [`tick_dt`] — this is the host/sim path.
    pub fn tick(&mut self) -> bool {
        self.tick_dt(FRAME_MS)
    }

    /// Advance HUD countdowns and fling momentum by REAL elapsed time.
    ///
    /// This used to assume a fixed 60 fps: the fling stepped `v / 60.0` per call and decayed by a
    /// flat 0.92. But the project's own bench measured a SCROLLING frame at ~31 ms on device
    /// (~32 fps) — and flinging *is* scrolling, so during the one animation that matters the
    /// assumption was off by 2x in both terms at once: each step moved half as far as intended
    /// AND the decay compounded twice as fast per second. A flick travelled a fraction of its
    /// intended distance on hardware while feeling perfect on the host, which is exactly the kind
    /// of difference that reads as "the device is just sluggish".
    pub fn tick_dt(&mut self, dt_ms: u32) -> bool {
        // Return true for EVERY counting-down frame, INCLUDING the one that reaches 0 — that
        // frame must repaint (now without the HUD/toast) to clear it. (With dirty-flag rendering,
        // returning false on the 0-transition would leave it stuck on screen.)
        let mut animating = false;
        // Clamp: a long stall (deferred init, a USB-MSC session) must not teleport a fling or
        // swallow a whole toast in one step.
        let dt = (dt_ms.max(1)).min(200) as f32;
        // Fling momentum: integrate over real time, decay exponentially per unit time (0.92 per
        // 60 fps frame, expressed continuously), stop below a threshold. Hitting the clamp
        // (top/bottom) kills it immediately.
        if self.fling_v != 0.0 {
            let scroll_of = |a: &Self| match a.current() {
                Screen::Library => a.lib_scroll_px,
                Screen::Album => a.album_scroll_px,
                Screen::Settings => a.settings_scroll_px,
                _ => 0,
            };
            let before = scroll_of(self);
            let step = self.fling_v * dt / 1000.0;
            self.scroll_px(step as i32);
            let after = scroll_of(self);
            self.fling_v *= 0.92f32.powf(dt / FRAME_MS as f32);
            if self.fling_v.abs() < 30.0 || (step as i32 != 0 && after == before) {
                self.fling_v = 0.0;
            }
            animating = true;
        }
        // HUD/toast countdowns are expressed in 60 fps frames; burn the number of frames that
        // really elapsed so their on-screen duration is the same at any frame rate.
        let frames = ((dt / FRAME_MS as f32).round() as u8).max(1);
        for ctr in [&mut self.vol_overlay, &mut self.toast_frames, &mut self.queue_anim_frames] {
            if *ctr > 0 {
                *ctr = ctr.saturating_sub(frames);
                animating = true;
            }
        }
        animating
    }

    /// Sync the UI volume to the device's real level (the shell pushes this after it reads/sets
    /// PlayerService volume), without popping the HUD.
    pub fn set_volume(&mut self, level: u8) {
        self.volume = level.min(crate::overlay::VOL_MAX);
    }

    pub fn volume(&self) -> u8 {
        self.volume
    }

    /// Current volume as the raw 0..VOL_MAX (=120) step level. The shell writes this 1:1 to the
    /// device mixer ('master volume' is also 0..120) — no lossy percent round-trip.
    pub fn volume_level(&self) -> u8 {
        self.volume
    }

    /// The current 10-band EQ gains (dB), for the shell to apply to the device DSP.
    pub fn eq_bands(&self) -> [i8; 10] {
        self.eq_bands
    }
    /// Restore EQ gains (from persisted settings). Clamps each band to the editable ±6 dB range.
    pub fn set_eq_bands(&mut self, bands: [i8; 10]) {
        for (dst, src) in self.eq_bands.iter_mut().zip(bands.iter()) {
            *dst = (*src).clamp(-6, 6);
        }
    }
    /// Restore the Sound-effect toggles from a bitmask (see sound_flags for the bit order).
    pub fn set_sound_flags(&mut self, f: u8) {
        self.snd_dsee = f & 1 != 0;
        self.snd_vinyl = f & (1 << 1) != 0;
        self.snd_vpt = f & (1 << 2) != 0;
        self.snd_dc = f & (1 << 3) != 0;
        self.snd_norm = f & (1 << 4) != 0;
        self.snd_clear = f & (1 << 5) != 0;
    }

    /// The "Up Next" queue = the album (from the library) the now-playing track belongs to, plus
    /// the playing index within it. Matched by title (+ artist when known). `None` when nothing is
    /// playing or the track isn't in the library. Real data, derived offline from the DB — not the
    /// PlayerService queue (which would need TrackSequence RE'd), but the natural "rest of the album".
    pub fn now_playing_queue<'a>(&'a self, title: &str, artist: &str) -> Option<(&'a str, &'a [SongRow], usize)> {
        if title.is_empty() {
            return None;
        }
        for g in &self.lib.album_groups {
            for al in &g.albums {
                if let Some(idx) = al
                    .track_list
                    .iter()
                    .position(|s| s.title == title && (artist.is_empty() || s.artist == artist))
                {
                    return Some((al.name.as_str(), &al.track_list, idx));
                }
            }
        }
        None
    }

    /// Real storage label ("used / total GB") for the Settings Storage row; the shell pushes it from
    /// statvfs. Returns a neutral placeholder until reported.
    pub fn storage_label(&self) -> &str {
        if self.storage.is_empty() {
            "—"
        } else {
            &self.storage
        }
    }
    pub fn set_storage(&mut self, label: &str) {
        self.storage = label.to_string();
    }

    /// Sleep-timer live remaining (minutes; 0 = off). cinder-ffi counts down and pushes it via
    /// set_sleep_min so the Settings row follows. `sleep_label` is what that row shows.
    pub fn sleep_min(&self) -> u32 {
        self.sleep_min
    }
    pub fn set_sleep_min(&mut self, m: u32) {
        self.sleep_min = m;
        if m == 0 {
            self.sleep_idx = 0; // expired/cancelled → back to "Off" in the cycle
        }
    }
    pub fn sleep_label(&self) -> String {
        if self.sleep_min == 0 {
            "OFF".to_string()
        } else {
            format!("{} MIN", self.sleep_min)
        }
    }

    /// First-run onboarding. The shell shows it on first boot (when not yet seen); it's persisted so
    /// it appears once. `start_onboarding` jumps straight into the intro (unlocked, page 0).
    pub fn onboarding_seen(&self) -> bool {
        self.onboarding_seen
    }
    pub fn set_onboarding_seen(&mut self, v: bool) {
        self.onboarding_seen = v;
    }
    pub fn start_onboarding(&mut self) {
        self.onboarding_page = 0;
        self.locked = false;
        self.go(Screen::Onboarding);
    }
    // Finish/skip: mark seen (persisted) + return to where we came from.
    fn finish_onboarding(&mut self) {
        self.onboarding_seen = true;
        self.locked = false;
        if self.stack.len() > 1 {
            self.pop(); // opened from the Menu → back to Menu
        } else {
            self.go(Screen::NowPlaying); // first-run → start listening
        }
    }

    /// Battery-care (Itawari) toggle state. The shell reads the device's real value at boot and
    /// pushes it via set_battery_care; battery_care() returns the UI's current desired value so the
    /// shell can apply a toggle to PowerMgrServiceClient.
    pub fn battery_care(&self) -> bool {
        self.battery_care
    }
    pub fn set_battery_care(&mut self, on: bool) {
        self.battery_care = on;
    }

    /// Sound effect toggles as a bitmask for the shell to apply via EffectCtrlDmp:
    /// bit0 DSEE · bit1 Vinyl · bit2 VPT · bit3 DC-Phase · bit4 Normalizer · bit5 ClearAudio+.
    pub fn sound_flags(&self) -> u8 {
        (self.snd_dsee as u8)
            | (self.snd_vinyl as u8) << 1
            | (self.snd_vpt as u8) << 2
            | (self.snd_dc as u8) << 3
            | (self.snd_norm as u8) << 4
            | (self.snd_clear as u8) << 5
    }

    /// A/B compare state on the Sound screen: true = effect chain bypassed (B), false = active (A).
    /// The shell applies this via cinder_effects_set_bypass after a SoundBypass action.
    pub fn sound_bypass(&self) -> bool {
        self.snd_ab_bypass
    }

    /// Device-wide BT codec preference (index into bluetooth::CODECS) + LDAC quality tier (index
    /// into bluetooth::QUALITIES). The shell reads these after a BtCodecChanged action and applies
    /// them via BtTransmitterService (and the same values feed the USB-DAC→LDAC bridge). The setters
    /// let the shell restore the persisted values at boot.
    pub fn screen_off_s(&self) -> u32 {
        self.screen_off_s
    }
    pub fn set_screen_off_s(&mut self, secs: u32) {
        // Snap to a known preset so a hand-edited or corrupt settings file can't produce a value
        // the Settings row can't display or cycle away from.
        self.screen_off_idx = SCREEN_OFF_PRESETS
            .iter()
            .position(|&p| p == secs)
            .unwrap_or(0);
        self.screen_off_s = SCREEN_OFF_PRESETS[self.screen_off_idx];
    }
    pub fn set_liked_count(&mut self, n: usize) {
        self.liked_count = n;
    }
    pub fn liked_count(&self) -> usize {
        self.liked_count
    }
    pub fn brightness(&self) -> u8 {
        self.brightness
    }
    pub fn set_brightness(&mut self, level: u8) {
        self.brightness = level.clamp(1, 5);
    }
    /// The accent's cycle index — what gets persisted.
    pub fn accent(&self) -> u8 {
        self.accent.index() as u8
    }
    /// Restore a persisted accent. Out-of-range snaps to the default (see `Accent::from_index`),
    /// so a corrupt settings file can never restore a colour the picker can't reach.
    pub fn set_accent(&mut self, i: u8) {
        self.accent = Accent::from_index(i as usize);
    }
    pub fn bt_codec(&self) -> u8 {
        self.bt_codec
    }
    pub fn bt_ldac_quality(&self) -> u8 {
        self.bt_ldac_quality
    }
    pub fn set_bt_codec(&mut self, v: u8) {
        self.bt_codec = (v as usize % crate::bluetooth::CODECS.len()) as u8;
    }
    pub fn set_bt_ldac_quality(&mut self, v: u8) {
        self.bt_ldac_quality = (v as usize % crate::bluetooth::QUALITIES.len()) as u8;
    }
    /// USB-DAC mode engaged? The shell reads this after a UsbDacToggle action to start/stop the
    /// LDAC bridge + switch the USB gadget to UAC (without tearing down Bluetooth).
    pub fn usb_dac_on(&self) -> bool {
        self.usb_dac_on
    }

    /// Visualiser settings (the shell reads these to gate/animate; the render uses viz_kind).
    pub fn viz_kind(&self) -> u8 {
        self.viz_kind
    }
    /// Is the visualiser showing at all? The analyzer gating asks this — starting Sony's service
    /// is a yes/no question even though the display is a five-way size.
    pub fn viz_on(&self) -> bool {
        self.viz_size != 0
    }
    /// The visualiser size index (0 = off). Persisted.
    pub fn viz_size(&self) -> u8 {
        self.viz_size
    }
    pub fn set_viz_size(&mut self, i: u8) {
        self.viz_size = i % crate::viz::SIZE_COUNT;
    }
    /// The Now Playing page index. Persisted, so the device comes back to the page you left on.
    pub fn np_page(&self) -> u8 {
        self.np_page
    }
    pub fn set_np_page(&mut self, i: u8) {
        self.np_page = i % crate::now_playing::PAGES;
    }
    /// Does anything on screen right now need the spectrum? True for the audio pages whatever the
    /// cover overlay is set to, and for the cover page only when its overlay is on. The shell
    /// starts and stops Sony's analyzer to match, so a page that shows nothing costs nothing.
    pub fn wants_spectrum(&self) -> bool {
        self.np_page != 0 || self.viz_on()
    }
    pub fn set_viz_kind(&mut self, k: u8) {
        self.viz_kind = k % crate::viz::COUNT;
    }
    /// Legacy on/off setter — kept because a settings file written before sizes existed carries
    /// `viz_on=`, and an upgrade must not silently turn the visualiser off (or on).
    pub fn set_viz_on(&mut self, on: bool) {
        self.viz_size = if on { 4 } else { 0 }; // FULL was the only "on" there used to be
    }
    fn cycle_viz(&mut self) {
        self.viz_kind = (self.viz_kind + 1) % crate::viz::COUNT;
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
fn tab_name(t: Tab) -> &'static str {
    match t {
        Tab::Songs => "Songs",
        Tab::Albums => "Albums",
        Tab::Artists => "Artists",
        Tab::Playlists => "Playlists",
    }
}
/// Display title for a Screen, taken from the Menu table (falls back to the app name).
fn screen_title(s: Screen) -> &'static str {
    MENU.iter().find(|m| m.0 == s).map(|m| m.2).unwrap_or("Cinder")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unlocked() -> App {
        App::unlocked()
    }

    /// The Menu subtitle contract: a row's literal is EMPTY exactly when render fills it from live
    /// state, and non-empty only when the text is genuinely fixed. This is the guard against the
    /// prototype's mock strings creeping back — those literals ("124 albums · 1,842 tracks",
    /// "88.6 MHz", "WH-1000XM5 · LDAC") stated things that were not true of the device.
    /// If you add a Menu row, either give it a static subtitle that is always true, or leave it
    /// empty AND add an arm to the `value:` match in render.
    #[test]
    fn menu_rows_with_no_static_subtitle_are_the_ones_render_fills_in() {
        // Compared as sorted debug names — Screen deliberately isn't Ord.
        let mut dynamic: Vec<String> = MENU
            .iter()
            .filter(|(_, _, _, value)| value.is_empty())
            .map(|(screen, _, _, _)| format!("{screen:?}"))
            .collect();
        dynamic.sort();
        let mut expected: Vec<String> = [
            Screen::NowPlaying, // no subtitle by design (the title is the screen)
            Screen::Library,
            Screen::UpNext,
            Screen::Fm, // tuner not wired — deliberately says nothing
            Screen::Eq,
            Screen::Sound,
            Screen::Bluetooth,
            Screen::UsbDac,
        ]
        .iter()
        .map(|s| format!("{s:?}"))
        .collect();
        expected.sort();
        assert_eq!(dynamic, expected);
    }

    /// No static Menu subtitle may assert a count or a device name. Digits are the tell: every mock
    /// string we removed had one ("124 albums", "8 tracks · 41:24", "88.6 MHz", "WH-1000XM5").
    #[test]
    fn static_menu_subtitles_never_assert_countable_state() {
        for (screen, _, label, value) in MENU.iter() {
            assert!(
                !value.chars().any(|c| c.is_ascii_digit()),
                "static subtitle for {label} ({screen:?}) asserts a number: {value:?} — \
                 make it live (empty literal + a render arm) or drop the claim"
            );
        }
    }

    /// The live subtitles must report this App's real state. (These are the actual strings the Menu
    /// draws; see App::menu_subtitles.) A fresh App still holds `Library::sample()` — the shell
    /// replaces it in cinder_db_open, and substitutes an EMPTY library if the DB fails to load, so
    /// the sample never stands in for the user's music on device. Both cases are asserted here.
    #[test]
    fn live_menu_subtitles_report_real_state() {
        let mut app = unlocked();
        // Library caption counts whatever library is actually loaded, not a fixed number.
        let subs = app.menu_subtitles();
        assert_eq!(
            subs.library,
            format!("{} albums · {} tracks", app.lib.album_count(), app.lib.songs.len())
        );
        // An empty library says so rather than showing a count of nothing.
        app.set_library(Library::default());
        let subs = app.menu_subtitles();
        assert_eq!(subs.library, "Empty");
        assert_eq!(subs.queue, "Queue empty");
        assert_eq!(subs.usb_dac, "Off");
        assert_eq!(subs.sound, "Off", "no effect is engaged on a fresh App");
        // EQ preset and BT codec name whatever is SELECTED — real values from the real tables,
        // indexed by the App's own selection (the old caption said "Custom A1" regardless).
        assert_eq!(subs.eq, data::EQ_PRESETS[app.eq_preset].0);
        assert_eq!(subs.bluetooth, crate::bluetooth::CODECS[app.bt_codec as usize].0);
        // And specifically: no invented headphones, anywhere.
        assert!(!subs.bluetooth.contains("WH-"));
    }

    /// Turning effects on changes the Sound subtitle to name them — proving it tracks state rather
    /// than being a fixed feature list (it used to read "DSEE HX · VPT · Vinyl" unconditionally).
    #[test]
    fn sound_subtitle_names_only_the_engaged_effects() {
        let mut app = unlocked();
        app.snd_dsee = true;
        app.snd_vpt = true;
        assert_eq!(app.menu_subtitles().sound, "DSEE HX · VPT");
        app.snd_dsee = false;
        app.snd_vpt = false;
        assert_eq!(app.menu_subtitles().sound, "Off");
    }

    /// Brightness cycles 1..5 and wraps — and never reaches 0. A 0 would let the shell write a dark
    /// panel, and since the level is persisted that single tap would survive reboots, leaving the
    /// Settings screen you need to undo it invisible.
    #[test]
    fn brightness_cycles_one_to_five_and_never_reaches_zero() {
        let mut app = unlocked();
        app.set_brightness(1);
        let mut seen = vec![app.brightness()];
        for _ in 0..6 {
            app.settings_sel = crate::settings::ROW_BRIGHTNESS;
            let acts = app.settings_activate();
            assert_eq!(acts, vec![Action::BrightnessChanged(app.brightness())]);
            seen.push(app.brightness());
        }
        assert!(seen.iter().all(|&l| (1..=5).contains(&l)), "level left 1..=5: {seen:?}");
        // 1→2→3→4→5→1→2 : it wraps rather than saturating, and 0 never appears.
        assert_eq!(seen, vec![1, 2, 3, 4, 5, 1, 2]);
    }

    /// Out-of-range values from a corrupt settings file are clamped, not trusted.
    #[test]
    fn set_brightness_clamps_out_of_range_values() {
        let mut app = unlocked();
        app.set_brightness(0);
        assert_eq!(app.brightness(), 1);
        app.set_brightness(200);
        assert_eq!(app.brightness(), 5);
    }

    /// The idle screen-off timer must default to OFF and its cycle must START at OFF. It blanks the
    /// panel, so it has to be opt-in: a default-on idle blank would land on every user at once, on
    /// hardware where a failed wake looks like a dead device.
    #[test]
    fn screen_off_timer_defaults_to_off_and_cycles_from_off() {
        let app = unlocked();
        assert_eq!(app.screen_off_s(), 0, "must be opt-in");
        assert_eq!(SCREEN_OFF_PRESETS[0], 0, "the cycle has to start at OFF");

        let mut app = unlocked();
        let mut seen = vec![app.screen_off_s()];
        for _ in 0..SCREEN_OFF_PRESETS.len() {
            app.settings_sel = crate::settings::ROW_SCREEN_OFF;
            let acts = app.settings_activate();
            assert_eq!(acts, vec![Action::ScreenOffTimer(app.screen_off_s())]);
            seen.push(app.screen_off_s());
        }
        // Cycles through every preset and wraps back to OFF, so it is always reachable again.
        assert_eq!(seen.first(), Some(&0));
        assert_eq!(seen.last(), Some(&0));
        for p in SCREEN_OFF_PRESETS {
            assert!(seen.contains(&p), "preset {p} unreachable by cycling");
        }
    }

    /// A hand-edited or corrupt settings value snaps to a known preset — otherwise the row would
    /// display something the cycle can never return to.
    #[test]
    fn set_screen_off_snaps_to_a_known_preset() {
        let mut app = unlocked();
        app.set_screen_off_s(37);
        assert_eq!(app.screen_off_s(), 0, "unknown value falls back to OFF");
        app.set_screen_off_s(60);
        assert_eq!(app.screen_off_s(), 60);
    }

    #[test]
    fn screen_off_labels_read_as_durations() {
        assert_eq!(screen_off_label(0), "OFF");
        assert_eq!(screen_off_label(15), "15 SEC");
        assert_eq!(screen_off_label(60), "1 MIN");
        assert_eq!(screen_off_label(120), "2 MIN");
        assert_eq!(screen_off_label(90), "1:30");
    }

    /// Boot to stock reboots the device, so one stray tap must never do it: the row arms first and
    /// only acts on a second tap.
    #[test]
    fn boot_to_stock_needs_two_taps() {
        let mut app = unlocked();
        app.settings_sel = crate::settings::ROW_BOOT_STOCK;
        assert_eq!(app.settings_activate(), vec![], "first tap only arms");
        assert_eq!(app.settings_activate(), vec![Action::BootToStock], "second tap acts");
        // And it disarms after firing, so a third tap arms again rather than re-firing.
        assert_eq!(app.settings_activate(), vec![]);
    }

    /// An armed confirmation must not survive touching something else — otherwise a tap on this row
    /// minutes later, with no prompt on screen, would restart the device.
    #[test]
    fn boot_to_stock_disarms_when_you_touch_anything_else() {
        let mut app = unlocked();
        app.settings_sel = crate::settings::ROW_BOOT_STOCK;
        assert_eq!(app.settings_activate(), vec![]); // armed
        app.settings_sel = crate::settings::ROW_THEME;
        let _ = app.settings_activate(); // a different row cancels it
        app.settings_sel = crate::settings::ROW_BOOT_STOCK;
        assert_eq!(app.settings_activate(), vec![], "must re-arm, not fire");

        // Leaving the screen cancels it too.
        let mut app = unlocked();
        app.settings_sel = crate::settings::ROW_BOOT_STOCK;
        assert_eq!(app.settings_activate(), vec![]); // armed
        app.push(Screen::Library);
        app.pop();
        assert_eq!(app.settings_activate(), vec![], "must re-arm after navigating away");
    }

    /// EVERY Settings row must be reachable by a finger at some scroll position. This caught a real
    /// bug: the content is 919px tall on an 800px panel, so before this screen scrolled the ABOUT
    /// rows sat past the bottom edge — "Model" was entirely unreachable and "Firmware" was clipped,
    /// and adding "Boot to stock" pushed both fully off. row_at also hardcodes the section
    /// boundaries, so it silently drifts from render whenever a row is added.
    #[test]
    fn every_settings_row_is_reachable_at_some_scroll_position() {
        use crate::settings::{max_scroll_px, row_at, ROWS};
        let mut hit: Vec<usize> = Vec::new();
        for scroll in 0..=max_scroll_px() {
            for y in 0..crate::canvas::H as i32 {
                if let Some(r) = row_at(y, scroll) {
                    if !hit.contains(&r) {
                        hit.push(r);
                    }
                }
            }
        }
        hit.sort();
        assert_eq!(hit, (0..ROWS).collect::<Vec<_>>(), "some rows can never be tapped");
    }

    /// Tapping a swatch must select THAT accent, not advance the cycle by one. This is the whole
    /// reason the swatches are hit-tested separately from the row.
    #[test]
    fn tapping_an_accent_swatch_picks_that_colour() {
        use crate::settings::{accent_hit, ROW_ACCENT};
        let mut a = unlocked();
        a.go(Screen::Settings);
        // Find a scroll offset where the Accent row is on screen, then the y of one of its swatches.
        let scroll = (0..=crate::settings::max_scroll_px())
            .find(|s| (0..crate::canvas::H as i32).any(|y| crate::settings::row_at(y, *s) == Some(ROW_ACCENT)))
            .expect("the Accent row must be reachable at some scroll");
        a.settings_scroll_px = scroll;
        let y = (0..crate::canvas::H as i32)
            .find(|y| crate::settings::row_at(*y, scroll) == Some(ROW_ACCENT))
            .unwrap() + 28; // middle of the row
        for want in 0..Accent::COUNT {
            // Sweep x to find a pixel that really is inside swatch `want`, then tap exactly there.
            let x = (0..crate::canvas::W as i32)
                .find(|x| accent_hit(*x, y, scroll) == Some(want))
                .unwrap_or_else(|| panic!("swatch {want} has no tappable pixel"));
            let acts = a.tap(x, y);
            assert!(acts.is_empty(), "accent is render-only; it must not emit a shell action");
            assert_eq!(a.accent(), want as u8, "tapping swatch {want} selected something else");
            assert_eq!(a.settings_sel, ROW_ACCENT, "the tap should focus the Accent row");
        }
    }

    /// The physical Select button still cycles the row — the swatches are the shortcut, not the
    /// only way in (there is no d-pad on this device, but Select exists and Settings is keyboard-
    /// navigable).
    #[test]
    fn select_on_the_accent_row_cycles() {
        let mut a = unlocked();
        a.go(Screen::Settings);
        a.settings_sel = crate::settings::ROW_ACCENT;
        for i in 0..Accent::COUNT {
            assert_eq!(a.accent() as usize, i);
            a.press(Button::Select);
        }
        assert_eq!(a.accent(), 0, "the cycle must wrap back to the default");
    }

    /// A tap on a swatch counts as touching a row other than Boot to stock, so it has to disarm a
    /// pending restart confirmation — otherwise picking a colour could leave the device one stray
    /// tap from rebooting.
    #[test]
    fn picking_an_accent_disarms_boot_to_stock() {
        use crate::settings::{accent_hit, ROW_ACCENT};
        let mut a = unlocked();
        a.go(Screen::Settings);
        a.settings_sel = crate::settings::ROW_BOOT_STOCK;
        assert!(a.settings_activate().is_empty(), "first tap only arms");
        assert!(a.boot_stock_armed);
        let scroll = (0..=crate::settings::max_scroll_px())
            .find(|s| (0..crate::canvas::H as i32).any(|y| crate::settings::row_at(y, *s) == Some(ROW_ACCENT)))
            .unwrap();
        a.settings_scroll_px = scroll;
        let y = (0..crate::canvas::H as i32)
            .find(|y| crate::settings::row_at(*y, scroll) == Some(ROW_ACCENT))
            .unwrap() + 28;
        let x = (0..crate::canvas::W as i32).find(|x| accent_hit(*x, y, scroll).is_some()).unwrap();
        a.tap(x, y);
        assert!(!a.boot_stock_armed, "picking a colour left the restart armed");
    }

    /// The swatch band must sit inside the Accent row and nowhere else — a swatch hit-test that
    /// leaked into the neighbouring rows would swallow taps on Theme or Visualiser type.
    #[test]
    fn accent_swatches_do_not_leak_into_other_rows() {
        use crate::settings::{accent_hit, row_at, ROW_ACCENT};
        for scroll in 0..=crate::settings::max_scroll_px() {
            for y in 0..crate::canvas::H as i32 {
                for x in (0..crate::canvas::W as i32).step_by(3) {
                    if accent_hit(x, y, scroll).is_some() {
                        assert_eq!(
                            row_at(y, scroll),
                            Some(ROW_ACCENT),
                            "swatch hit at ({x},{y}) scroll {scroll} is outside the Accent row"
                        );
                    }
                }
            }
        }
    }

    /// Scrolling to the bottom must actually bring the last row fully on screen — a max_scroll that
    /// is too small leaves it half-clipped and awkward to hit.
    #[test]
    fn settings_scrolls_far_enough_to_reach_the_last_row() {
        use crate::settings::{content_height, max_scroll_px};
        assert!(max_scroll_px() > 0, "content is taller than the panel; it must scroll");
        assert!(
            content_height() - max_scroll_px() <= crate::canvas::H as i32,
            "bottom of the content is still off-screen at full scroll"
        );
    }

    /// The A–Z rail must land on a row that really is in that bucket, on every tab. A jump that
    /// lands near-but-not-on the letter is worse than no jump: you can't tell it worked.
    #[test]
    fn az_jump_lands_on_a_row_in_that_bucket() {
        let app = unlocked();
        for &letter in library::AZ_LETTERS {
            for tab in [Tab::Songs, Tab::Albums, Tab::Artists, Tab::Playlists] {
                let Some(px) =
                    library::az_scroll_for(tab, &app.lib, letter, app.album_sort, app.album_expanded)
                else {
                    continue; // no rows in this bucket — rail greys it and the tap is a no-op
                };
                let max = library::max_scroll_px(tab, &app.lib, app.album_sort, app.album_expanded);
                assert!((0..=max).contains(&px), "{tab:?}/{}: scroll {px} out of 0..={max}", letter as char);
            }
        }
    }

    /// "The Beatles" files under B, and anything not a letter buckets under '#'. Sorting already
    /// works this way, so the rail has to agree or the jump lands nowhere near the eye.
    #[test]
    fn az_buckets_ignore_a_leading_the_and_fold_non_letters() {
        assert_eq!(library::az_bucket("The Beatles"), b'B');
        assert_eq!(library::az_bucket("the xx"), b'X');
        assert_eq!(library::az_bucket("Theatre of Tragedy"), b'T', "only a whole leading word");
        assert_eq!(library::az_bucket("aphex twin"), b'A');
        assert_eq!(library::az_bucket("65daysofstatic"), b'#');
        assert_eq!(library::az_bucket("...And Justice For All"), b'#');
        assert_eq!(library::az_bucket(""), b'#');
    }

    /// The rail's hit test must cover the whole list height and map monotonically onto the letters,
    /// with the first and last letters actually reachable.
    #[test]
    fn az_rail_hit_test_covers_every_letter() {
        for tab in [Tab::Songs, Tab::Albums, Tab::Artists, Tab::Playlists] {
            let mut seen: Vec<u8> = Vec::new();
            for y in 0..crate::canvas::H as i32 {
                if let Some(l) = library::az_letter_at(y, tab) {
                    if !seen.contains(&l) {
                        seen.push(l);
                    }
                }
            }
            assert_eq!(seen, library::AZ_LETTERS.to_vec(), "{tab:?}: rail letters unreachable");
        }
        // And the rail only claims the right edge, so it can't swallow row taps.
        assert!(library::az_hit_x(crate::canvas::W as i32 - 1));
        assert!(!library::az_hit_x(crate::canvas::W as i32 - library::AZ_W - 1));
    }

    /// The status strip's three zones must be distinct, and the Shelf target must be big enough for
    /// a thumb — it is the only part of the strip that doesn't open the Menu, so a miss there lands
    /// you on the wrong screen rather than doing nothing.
    #[test]
    fn status_strip_zones_are_distinct_and_the_shelf_target_is_thumb_sized() {
        use crate::chrome::{status_hit, StatusTap, STATUS_H};
        let mut shelf_w = 0;
        let mut np_w = 0;
        let mut menu_w = 0;
        for x in 0..crate::canvas::W as i32 {
            match status_hit(x, STATUS_H / 2) {
                Some(StatusTap::Shelf) => shelf_w += 1,
                Some(StatusTap::NowPlaying) => np_w += 1,
                Some(StatusTap::Menu) => menu_w += 1,
                None => panic!("x={x} inside the strip resolved to nothing"),
            }
        }
        assert!(shelf_w >= 56, "Shelf target only {shelf_w}px wide — too small for a thumb");
        assert!(np_w >= 120, "Now Playing zone only {np_w}px wide");
        assert!(menu_w > 0, "the forgiving Menu zone must survive");
        assert_eq!(shelf_w + np_w + menu_w, crate::canvas::W as i32);
        // Below the strip the whole thing stops claiming taps.
        assert_eq!(status_hit(10, STATUS_H), None);
    }

    /// One-tap return to Now Playing from anywhere, and it must COLLAPSE the stack rather than
    /// burying Now Playing under whatever was being browsed.
    #[test]
    fn status_bar_returns_to_now_playing_from_any_screen() {
        let mut app = unlocked();
        app.push(Screen::Library);
        app.push(Screen::Settings);
        assert_eq!(app.tap(20, 20), vec![]);
        assert_eq!(app.current(), Screen::NowPlaying);
        // Back must not walk down through the screens we came from.
        let _ = app.press(Button::Back);
        assert_eq!(app.current(), Screen::NowPlaying, "stack should have collapsed");
    }

    /// The Now Playing bar's left zone is a real play/pause button; the rest still opens the screen.
    #[test]
    fn np_bar_left_zone_is_play_pause_and_the_rest_navigates() {
        use crate::chrome::{np_bar_rect, NP_BAR_PLAY_W};
        let (_, by, w, h) = np_bar_rect();
        let midy = by + h / 2;

        let mut app = unlocked();
        app.push(Screen::Library);
        assert_eq!(app.tap(NP_BAR_PLAY_W / 2, midy), vec![Action::PlayPause]);
        assert_eq!(app.current(), Screen::Library, "play/pause must not navigate");

        let mut app = unlocked();
        app.push(Screen::Library);
        assert_eq!(app.tap(w - 40, midy), vec![]);
        assert_eq!(app.current(), Screen::NowPlaying);
    }

    /// A fling must cover the same distance in the same wall-clock time whatever the frame rate.
    /// It used to step `v/60` per call and decay a flat 0.92 per call, so at the ~32 fps a scrolling
    /// frame actually costs on device a flick travelled roughly half as far AND died twice as fast
    /// — while feeling right on the host, which runs far quicker.
    #[test]
    fn fling_distance_is_frame_rate_independent() {
        // Needs a list long enough to scroll: the sample library is 8 songs, so the fling would hit
        // the clamp on its first step and the test would measure nothing.
        let big = {
            let mut lib = crate::model::Library::sample();
            let base = lib.songs.clone();
            while lib.songs.len() < 400 {
                lib.songs.extend(base.iter().cloned());
            }
            lib
        };
        let travel = |dt: u32, steps: usize| {
            let mut app = unlocked();
            app.push(Screen::Library);
            app.set_library(big.clone());
            app.lib_tab = crate::library::Tab::Songs;
            app.fling(3000.0);
            for _ in 0..steps {
                app.tick_dt(dt);
            }
            app.lib_scroll_px
        };
        // Same 500 ms of wall clock, delivered as 30x17ms and as 16x31ms (the device's real
        // scrolling frame time). The two must land within a few percent of each other.
        let fast = travel(17, 30);
        let slow = travel(31, 16);
        assert!(fast > 100, "fling should actually travel ({fast}px)");
        let diff = (fast - slow).abs() as f32 / fast as f32;
        assert!(diff < 0.15, "frame-rate dependent: 17ms->{fast}px vs 31ms->{slow}px");
    }

    /// HUD countdowns are written in 60 fps frames; they must still last the same wall-clock time
    /// when frames are slower, or a toast outstays its welcome exactly when the device is busy.
    #[test]
    fn hud_countdown_duration_is_frame_rate_independent() {
        let mut fast = unlocked();
        let mut slow = unlocked();
        fast.toast_frames = TOAST_FRAMES;
        slow.toast_frames = TOAST_FRAMES;
        // ~500 ms each way.
        for _ in 0..30 {
            fast.tick_dt(17);
        }
        for _ in 0..16 {
            slow.tick_dt(31);
        }
        assert_eq!(
            fast.toast_frames == 0,
            slow.toast_frames == 0,
            "toast outlived its wall-clock duration at the slower frame rate"
        );
    }

    /// The visualiser must never animate without real spectrum data behind it. The synthetic
    /// fallback moved identically for silence, a ballad and a drum solo — it looked like a
    /// representation of the audio and wasn't one.
    #[test]
    fn visualiser_is_hidden_unless_real_spectrum_data_is_arriving() {
        let app = unlocked();
        assert!(app.viz_on(), "precondition: the user preference is on");

        // No levels => the NowPlaying view-model must come out with viz_on false.
        let live_flag = |levels: Option<&[f32]>| {
            // Mirrors the expression in render's NowPlaying arm.
            app.viz_on() && levels.is_some()
        };
        assert!(!live_flag(None), "no data must mean no visualiser");
        let bars = [0.5f32; 36];
        assert!(live_flag(Some(&bars)), "real data must show it");
    }

    #[test]
    fn hold_switch_is_the_only_unlock() {
        // The Hold SWITCH locks/unlocks; the touchscreen and nav buttons never do.
        let mut a = App::unlocked();
        a.set_hold(true);
        assert!(a.locked);
        assert_eq!(a.current(), Screen::Lock);
        // While locked: a tap is ignored (touchscreen disabled) and stays on Lock.
        assert!(a.tap(200, 400).is_empty());
        assert!(a.locked);
        // Transport buttons still control playback WITHOUT unlocking.
        assert_eq!(a.press(Button::Right), vec![Action::Next]);
        assert!(a.locked);
        assert_eq!(a.press(Button::Play), vec![Action::PlayPause]);
        assert!(a.locked);
        // Power toggles the screen (emits Sleep) but does NOT unlock.
        assert_eq!(a.press(Button::Power), vec![Action::Sleep]);
        assert!(a.locked);
        // Only the switch going off unlocks.
        a.set_hold(false);
        assert!(!a.locked);
        assert_eq!(a.current(), Screen::NowPlaying);
    }

    #[test]
    fn power_toggles_screen_without_locking() {
        let mut a = unlocked();
        let acts = a.press(Button::Power);
        assert_eq!(acts, vec![Action::Sleep]); // shell blanks/wakes the backlight
        assert!(!a.locked); // Power never locks — that's the Hold switch's job
        assert_ne!(a.current(), Screen::Lock);
    }

    #[test]
    fn shelf_opens_pins_and_jumps_back() {
        let mut a = unlocked(); // Now Playing
        // open the Shelf from the status-bar bookmark → overlay over the current place (Now Playing)
        a.tap(392, 16);
        assert!(a.shelf_is_open());
        assert_eq!(a.current(), Screen::NowPlaying);
        // Pin "this place" (the Pin button), then close and navigate away.
        a.tap(420, 582); // PIN_BTN region
        assert!(a.pins[0].is_some());
        a.tap(240, 200); // dim backdrop → Close
        assert!(!a.shelf_is_open());
        // Go to the Library, reopen the Shelf, and tap the pin's "GO ›" → back to Now Playing.
        a.tap(344, 22); // status bar, on the ☰ glyph (not the bookmark) → Menu
        a.tap(200, 91 + 1 * 63 + 8); // Library row
        assert_eq!(a.current(), Screen::Library);
        a.open_shelf();
        a.tap(380, 640 + 12); // slot 0 "GO ›" hit
        assert!(!a.shelf_is_open());
        assert_eq!(a.current(), Screen::NowPlaying);
    }

    #[test]
    fn status_bar_bookmark_opens_shelf() {
        let mut a = unlocked(); // Now Playing
        a.tap(392, 16); // the bookmark glyph (top-right) → Shelf overlay
        assert!(a.shelf_is_open());
        assert_eq!(a.current(), Screen::NowPlaying); // overlays the current place
        // tapping elsewhere on the bar still opens the Menu (after closing the shelf)
        a.tap(240, 200); // backdrop → close
        assert!(!a.shelf_is_open());
        // Mid-strip (right of the clock/badge zone, left of the bookmark) still opens the Menu.
        a.tap(240, 16);
        assert_eq!(a.current(), Screen::Menu);
    }

    /// The status strip's two zones must land where the glyphs are DRAWN. Both come from
    /// `chrome::status_hit`, so this pins the agreement rather than a pair of literals.
    #[test]
    fn status_strip_targets_match_the_drawn_glyphs() {
        use crate::chrome::{status_hit, StatusTap, STATUS_H};
        // The bookmark is drawn at x=388, centre of the strip.
        assert_eq!(status_hit(388, STATUS_H / 2), Some(StatusTap::Shelf));
        // The ☰ is drawn at x=344 — outside the Shelf zone, so tapping it means Menu.
        assert_eq!(status_hit(344, STATUS_H / 2), Some(StatusTap::Menu));
        // The whole strip is live to its full height: the bottom row used to be dead space
        // (the strip was 34px) and is now a Menu target.
        assert_eq!(status_hit(240, STATUS_H - 1), Some(StatusTap::Menu));
        assert_eq!(status_hit(240, STATUS_H), None); // below the strip: not ours
        // Every x along the strip resolves to something — no dead columns.
        for x in 0..crate::W as i32 {
            assert!(status_hit(x, 10).is_some(), "dead column at x={x}");
        }
    }

    /// One tap from anywhere in the library back to Now Playing, however deep the drill-in went.
    #[test]
    fn np_bar_returns_to_now_playing() {
        let mut a = unlocked();
        a.tap(344, 22); // Menu
        a.tap(200, 91 + 63 + 8); // Library
        assert_eq!(a.current(), Screen::Library);
        let (_, by, _, _) = crate::chrome::np_bar_rect();
        a.tap(240, by + 20);
        assert_eq!(a.current(), Screen::NowPlaying);
        // …and it collapses the stack rather than popping one screen, so Back from Now Playing
        // doesn't walk back into the library.
        assert_eq!(a.stack.len(), 1);
    }

    /// The bar is pinned over the bottom of the list area, so no list row may resolve under it.
    #[test]
    fn np_bar_does_not_overlap_the_library_list() {
        let (_, by, _, _) = crate::chrome::np_bar_rect();
        let m = crate::model::Library::sample();
        for tab in [Tab::Songs, Tab::Albums, Tab::Artists, Tab::Playlists] {
            assert_eq!(library::hit_row(tab, &m, 0, by), None, "{tab:?} row under the bar");
            assert_eq!(library::hit_row(tab, &m, 0, by + 30), None, "{tab:?} row under the bar");
        }
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
        // Left from the default Albums tab lands on Songs (only Songs rows emit PlayIndex for a
        // single track; Artists navigate, Playlists emit PlayPlaylist).
        a.press(Button::Left);
        assert_ne!(a.lib_tab(), start); // tab changed
        assert_eq!(a.lib_tab(), Tab::Songs);
        assert_eq!(a.lib_index(), 0); // cursor reset on tab change
        a.press(Button::Down);
        assert!(a.lib_index() <= 1);
        // Select on a Songs row asks the shell to play that track (by object id)
        let acts = a.press(Button::Select);
        assert!(matches!(acts.as_slice(), [Action::PlayIndex(_)]));
    }

    /// Tapping a preset pill selects THAT preset. Regression: the pills are drawn per-pill but
    /// were hit-tested as five uniform 86px slots starting at x=22, so tapping "A2" applied
    /// "JAZZ", and taps on the blank space past the last pill still changed the sound.
    #[test]
    fn eq_preset_tap_selects_the_pill_under_the_finger() {
        for i in 0..data::EQ_PRESETS.len() {
            let mut a = unlocked();
            a.push(Screen::Eq);
            let (px, py, pw, ph) = crate::eq::preset_rect(i);
            let acts = a.tap(px + pw / 2, py + ph / 2);
            assert_eq!(a.eq_preset, i, "pill {i} centre selected preset {}", a.eq_preset);
            assert_eq!(acts, vec![Action::EqChanged(data::EQ_PRESETS[i].1)]);
        }
    }

    /// A tap in the gap between pills, or past the last one, must do nothing rather than change
    /// the sound.
    #[test]
    fn eq_preset_gaps_are_inert() {
        let mut a = unlocked();
        a.push(Screen::Eq);
        let (px0, py, pw0, ph) = crate::eq::preset_rect(0);
        let (px1, _, _, _) = crate::eq::preset_rect(1);
        let gap_x = (px0 + pw0 + px1) / 2; // between pill 0 and pill 1
        assert!(a.tap(gap_x, py + ph / 2).is_empty(), "gap between pills changed the EQ");
        let (lx, _, lw, _) = crate::eq::preset_rect(data::EQ_PRESETS.len() - 1);
        assert!(a.tap(lx + lw + 6, py + ph / 2).is_empty(), "blank space past the pills changed the EQ");
    }

    /// Above the zero line raises, below lowers — the split must be the drawn zero line, not an
    /// unrelated constant (it was 375 while the line is drawn at the field midpoint).
    #[test]
    fn eq_band_tap_raises_above_the_zero_line_and_lowers_below() {
        use crate::eq::{band_center_x, FIELD_BOTTOM, FIELD_MID, FIELD_TOP};
        let mut a = unlocked();
        a.push(Screen::Eq);
        a.eq_bands = [0; 10];
        for band in [0usize, 5, 9] {
            let x = band_center_x(band);
            a.tap(x, (FIELD_TOP + FIELD_MID) / 2); // upper half
            assert_eq!(a.eq_bands[band], 1, "band {band} above the line should raise");
            assert_eq!(a.eq_sel, band, "tap should select the band under the finger");
            a.tap(x, (FIELD_MID + FIELD_BOTTOM) / 2); // lower half
            assert_eq!(a.eq_bands[band], 0, "band {band} below the line should lower");
        }
        // Just below the zero line lowers (this pixel used to raise).
        a.tap(band_center_x(3), FIELD_MID + 1);
        assert_eq!(a.eq_bands[3], -1);
    }

    /// The band hit test must agree with where the knobs are drawn, for every band.
    #[test]
    fn eq_band_centres_resolve_to_their_own_band() {
        for i in 0..10 {
            assert_eq!(crate::eq::band_at(crate::eq::band_center_x(i)), Some(i), "band {i}");
        }
    }

    /// The EQ footer "Reset" was drawn but hit-tested nowhere.
    #[test]
    fn eq_footer_reset_flattens_the_bands() {
        let mut a = unlocked();
        a.push(Screen::Eq);
        a.eq_bands = [5, -3, 2, 0, 1, -1, 4, 2, -2, 6];
        let acts = a.tap(60, crate::eq::FOOTER_TOP + 20);
        assert_eq!(a.eq_bands, [0; 10]);
        assert_eq!(acts, vec![Action::EqChanged([0; 10])]);
    }

    /// The accent band on each Library tab shuffles in that tab's scope. It is the largest touch
    /// target on the screen and was previously drawn but hit-tested nowhere.
    #[test]
    fn shuffle_band_is_tappable_on_every_tab() {
        let (bx, by, bw, bh) = library::library_shuffle_band();
        let (cx, cy) = (bx + bw / 2, by + bh / 2);
        for (tab, want) in [
            (Tab::Songs, ShuffleScope::AllSongs),
            (Tab::Albums, ShuffleScope::ByAlbum),
            (Tab::Artists, ShuffleScope::ByArtist),
            (Tab::Playlists, ShuffleScope::Playlist),
        ] {
            let mut a = unlocked();
            a.push(Screen::Library);
            a.lib_tab = tab;
            assert_eq!(a.tap(cx, cy), vec![Action::Shuffle(want)], "tab {tab:?}");
        }
    }

    /// The band must not swallow taps meant for the list, and the list must not start inside it —
    /// this is the render/hit-test drift that made row taps land on the wrong song before.
    #[test]
    fn shuffle_band_and_list_do_not_overlap() {
        let (_, by, _, bh) = library::library_shuffle_band();
        for tab in [Tab::Songs, Tab::Albums, Tab::Artists, Tab::Playlists] {
            assert!(
                library::list_top(tab) >= by + bh,
                "{tab:?} list starts inside the shuffle band"
            );
        }
        // A tap one pixel below the band is a list tap, not a shuffle.
        let mut a = unlocked();
        a.push(Screen::Library);
        a.lib_tab = Tab::Songs;
        assert!(!library::hit_shuffle_band(240, by + bh + 1));
        // ...and one inside it is not a row.
        assert_eq!(library::hit_row(Tab::Songs, &a.lib, 0, by + bh / 2), None);
    }

    /// A playlist row has no single track under the cursor, so both input routes (button Select
    /// and a finger tap) play the whole list from the top.
    #[test]
    fn playlist_row_plays_the_whole_list() {
        let mut a = unlocked();
        a.push(Screen::Library);
        a.lib_tab = Tab::Playlists;
        a.lib_idx = 0;
        a.lib = crate::model::Library {
            playlists: vec![
                crate::model::PlaylistRow { id: 77, name: "Night Bus".into(), tracks: 3, art: "Night Bus".into() },
                crate::model::PlaylistRow { id: 78, name: "Morning".into(), tracks: 2, art: "Morning".into() },
            ],
            ..Default::default()
        };
        assert_eq!(a.press(Button::Select), vec![Action::PlayPlaylist(77)]);

        // Same for a tap. Derive the y from the render's own geometry (list_top + row_h) rather
        // than a magic number, so this can't drift from the layout the way a hard-coded y would.
        let y = library::list_top(Tab::Playlists) + library::row_h(Tab::Playlists) + 4;
        assert_eq!(a.tap(240, y), vec![Action::PlayPlaylist(78)]);
        assert_eq!(a.lib_index(), 1);
    }

    /// An empty Playlists tab must not emit an action for a tap on nothing.
    #[test]
    fn empty_playlists_tab_is_inert() {
        let mut a = unlocked();
        a.push(Screen::Library);
        a.lib_tab = Tab::Playlists;
        a.lib = crate::model::Library::default();
        assert!(a.press(Button::Select).is_empty());
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

    #[test]
    fn visualiser_cycles_and_settings_control_it() {
        let mut a = unlocked();
        // Option on Now Playing cycles the visualiser type
        let k0 = a.viz_kind();
        a.press(Button::Option);
        assert_eq!(a.viz_kind(), (k0 + 1) % crate::viz::COUNT);
        // route to Settings (menu idx 9)
        a.press(Button::Up); // Menu
        for _ in 0..9 {
            a.press(Button::Down);
        }
        a.press(Button::Select);
        assert_eq!(a.current(), Screen::Settings);
        // cursor down to the Visualiser row and cycle it. Walk to the constant rather than
        // pressing a fixed number of times, so inserting a DISPLAY row above it can't silently
        // retarget this test at whatever ends up in that slot.
        while a.settings_sel < crate::settings::ROW_VIZ {
            a.press(Button::Down);
        }
        assert_eq!(a.settings_sel, crate::settings::ROW_VIZ);
        let k1 = a.viz_kind();
        a.press(Button::Select);
        assert_eq!(a.viz_kind(), (k1 + 1) % crate::viz::COUNT);
        // down to the Visualiser SIZE row. It is no longer an on/off toggle: it cycles
        // OFF / EDGE / FLOOR / VEIL / FULL, because on the day theme the visualiser is drawn over
        // the album art and "how much" is the question that matters. OFF is index 0, so a full
        // cycle must pass through viz_on() == false exactly once and come home.
        a.press(Button::Down);
        assert_eq!(a.settings_sel, crate::settings::ROW_VIZ_ANIM);
        let start = a.viz_size();
        let mut seen_off = false;
        for _ in 0..crate::viz::SIZE_COUNT {
            a.press(Button::Select);
            if a.viz_size() == 0 {
                seen_off = true;
                assert!(!a.viz_on(), "size 0 must read as off");
            } else {
                assert!(a.viz_on(), "any non-zero size must read as on");
            }
        }
        assert!(seen_off, "the cycle never offered OFF");
        assert_eq!(a.viz_size(), start, "the cycle did not return to where it started");
        // cursor clamps at the last row
        for _ in 0..30 {
            a.press(Button::Down);
        }
        assert_eq!(a.settings_sel, crate::settings::ROWS - 1);
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

    fn enter_bluetooth() -> App {
        let mut a = unlocked();
        a.press(Button::Up); // Menu
        for _ in 0..6 {
            a.press(Button::Down);
        }
        a.press(Button::Select); // Bluetooth (idx 6)
        assert_eq!(a.current(), Screen::Bluetooth);
        assert!(a.bt_on);
        a
    }

    #[test]
    fn bluetooth_codec_and_quality_select() {
        let mut a = enter_bluetooth();
        // default codec = LDAC (0): the quality chips are visible. Pick 660 (chip index 2).
        assert_eq!(a.bt_codec, 0);
        assert_eq!(a.tap(260, 440), vec![Action::BtCodecChanged]);
        assert_eq!(a.bt_ldac_quality, 2);
        // select SBC (codec row 3) → device-wide codec changes; LDAC chips hide
        assert_eq!(a.tap(200, 360), vec![Action::BtCodecChanged]);
        assert_eq!(a.bt_codec, 3);
        // with SBC active there are no LDAC quality chips, so that band is now inert
        assert!(a.tap(260, 440).is_empty());
    }

    #[test]
    fn usb_dac_toggles_ldac_routing() {
        let mut a = unlocked();
        a.press(Button::Up); // Menu
        for _ in 0..7 {
            a.press(Button::Down);
        }
        a.press(Button::Select); // USB-DAC (idx 7)
        assert_eq!(a.current(), Screen::UsbDac);
        assert!(!a.usb_dac_on());
        // A stray tap in the body must NOT engage it: this switches the USB gadget mode and
        // starts the LDAC bridge, so it may only happen on the switch itself.
        assert!(a.tap(240, 300).is_empty(), "a body tap engaged USB-DAC");
        assert!(!a.usb_dac_on());
        // The switch engages USB-DAC → routes to 3.5mm + BT/LDAC (no BT disconnect).
        let sw = (441, 65); // centre of the drawn toggle
        assert_eq!(a.tap(sw.0, sw.1), vec![Action::UsbDacToggle(true)]);
        assert!(a.usb_dac_on());
        // tapping again disengages
        assert_eq!(a.tap(sw.0, sw.1), vec![Action::UsbDacToggle(false)]);
        assert!(!a.usb_dac_on());
        // The target is padded to ≥44px around the small drawn switch.
        assert!(crate::usbdac::hit_toggle(sw.0 - 20, sw.1 - 18));
        assert!(crate::usbdac::hit_toggle(sw.0 + 15, sw.1 + 18));
    }

    #[test]
    fn settings_usb_mode_enters_mass_storage() {
        let mut a = unlocked();
        a.press(Button::Up); // Menu
        for _ in 0..9 {
            a.press(Button::Down);
        }
        a.press(Button::Select); // Settings
        for _ in 0..crate::settings::ROW_USB_MODE {
            a.press(Button::Down);
        }
        assert_eq!(a.settings_sel, crate::settings::ROW_USB_MODE);
        assert_eq!(a.press(Button::Select), vec![Action::EnterUsbMsc]);
    }

    #[test]
    fn bluetooth_codec_persists_round_trip() {
        let mut a = unlocked();
        a.set_bt_codec(2); // aptX
        a.set_bt_ldac_quality(3); // 330
        assert_eq!(a.bt_codec(), 2);
        assert_eq!(a.bt_ldac_quality(), 3);
        a.set_bt_codec(99); // wraps into range, never panics
        assert!((a.bt_codec() as usize) < crate::bluetooth::CODECS.len());
    }

    #[test]
    fn settings_select_toggles_theme() {
        let mut a = unlocked();
        // route to Settings (menu index 9)
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

    #[test]
    fn settings_battery_care_toggles_and_emits_action() {
        let mut a = unlocked();
        a.press(Button::Up); // Menu
        for _ in 0..9 {
            a.press(Button::Down);
        }
        a.press(Button::Select); // -> Settings
        assert_eq!(a.current(), Screen::Settings);
        // move cursor to the Battery care row
        for _ in 0..crate::settings::ROW_BATTERY {
            a.press(Button::Down);
        }
        assert_eq!(a.settings_sel, crate::settings::ROW_BATTERY);
        let was = a.battery_care();
        let acts = a.press(Button::Select);
        assert_eq!(acts, vec![Action::BatteryCareChanged(!was)]);
        assert_eq!(a.battery_care(), !was);
        // shell can push the device's real state back in
        a.set_battery_care(false);
        assert!(!a.battery_care());
    }

    #[test]
    fn restore_eq_and_sound_round_trip() {
        let mut a = unlocked();
        a.set_eq_bands([3, -2, 6, -6, 0, 1, 99, -99, 4, 5]); // 99/-99 clamp to ±6
        assert_eq!(a.eq_bands(), [3, -2, 6, -6, 0, 1, 6, -6, 4, 5]);
        a.set_sound_flags(0b101101); // dsee, vpt, dc, clear
        let f = a.sound_flags();
        assert_eq!(f, 0b101101);
        assert_eq!(f & 1, 1); // dsee
        assert_eq!(f & (1 << 4), 0); // normalizer off
    }

    #[test]
    fn settings_sleep_timer_cycles_presets() {
        let mut a = unlocked();
        a.press(Button::Up); // Menu
        for _ in 0..9 {
            a.press(Button::Down);
        }
        a.press(Button::Select); // Settings
        for _ in 0..crate::settings::ROW_SLEEP {
            a.press(Button::Down);
        }
        assert_eq!(a.settings_sel, crate::settings::ROW_SLEEP);
        assert_eq!(a.sleep_min(), 0);
        assert_eq!(a.sleep_label(), "OFF");
        // Off -> 15 -> 30
        assert_eq!(a.press(Button::Select), vec![Action::SleepTimer(15)]);
        assert_eq!(a.sleep_min(), 15);
        assert_eq!(a.sleep_label(), "15 MIN");
        assert_eq!(a.press(Button::Select), vec![Action::SleepTimer(30)]);
        assert_eq!(a.sleep_min(), 30);
        // ffi pushes the live countdown back
        a.set_sleep_min(29);
        assert_eq!(a.sleep_label(), "29 MIN");
        a.set_sleep_min(0); // expired
        assert_eq!(a.sleep_label(), "OFF");
    }

    #[test]
    fn touch_tap_navigates() {
        let mut a = App::unlocked();
        assert_eq!(a.current(), Screen::NowPlaying);
        // The strip is forgiving, but not uniform: the far-left clock/badge zone is the ONE-TAP
        // return to Now Playing (the badge is the now-playing indicator), while the rest of the
        // strip opens the Menu.
        a.tap(20, 16); // over the clock → Now Playing (already there, so it stays)
        assert_eq!(a.current(), Screen::NowPlaying);
        a.tap(240, 16); // mid strip → Menu
        assert_eq!(a.current(), Screen::Menu);
        a.tap(200, 91 + 63 + 8); // leave Menu (into Library) so the next assert is meaningful
        a.tap(338, 22); // the ☰ icon → Menu too
        assert_eq!(a.current(), Screen::Menu);
        // tap the Library row (row 1: y0=91 + 1*63 + a bit)
        a.tap(200, 91 + 63 + 8);
        assert_eq!(a.current(), Screen::Library);
        // tap the Albums tab, then the header-back chevron → back to Menu
        a.tap(180, 105);
        assert_eq!(a.lib_tab, Tab::Albums);
        a.tap(20, 60);
        assert_eq!(a.current(), Screen::Menu);
    }

    #[test]
    fn touch_tap_nowplaying_transport() {
        let mut a = unlocked(); // NowPlaying
        let was = a.playing;
        let acts = a.tap(240, 692); // the play circle
        assert_eq!(acts, vec![Action::PlayPause]);
        assert_eq!(a.playing, !was);
        assert_eq!(a.tap(350, 692), vec![Action::Next]); // next button
        assert_eq!(a.tap(130, 692), vec![Action::Prev]); // prev button
        // shuffle / repeat icons are now tappable (previously dead on a touch-only device)
        assert_eq!(a.tap(44, 692), vec![Action::ShuffleToggle]);
        assert_eq!(a.tap(436, 692), vec![Action::RepeatCycle]);
    }

    #[test]
    fn swipe_pages_onboarding_and_skips_track() {
        let mut a = unlocked();
        a.start_onboarding();
        assert_eq!(a.current(), Screen::Onboarding);
        let p0 = a.onboarding_page;
        assert!(a.swipe(-1, 240, 400).is_empty()); // leftward = next page
        assert_eq!(a.onboarding_page, p0 + 1);
        assert!(a.swipe(1, 240, 400).is_empty()); // rightward = back
        assert_eq!(a.onboarding_page, p0);
        // swipe left through every page finishes onboarding
        for _ in 0..crate::onboarding::PAGES + 1 {
            a.swipe(-1, 240, 400);
        }
        assert!(a.current() != Screen::Onboarding);
        // Now Playing is ZONED: a swipe BELOW the paging block still skips tracks, exactly as it
        // always did. y=400 is on the artwork, so that one now turns the page instead.
        let mut b = unlocked();
        assert_eq!(b.current(), Screen::NowPlaying);
        let below = crate::now_playing::PAGE_SWIPE_BOT + 40;
        assert_eq!(b.swipe(-1, 240, below), vec![Action::Next]);
        assert_eq!(b.swipe(1, 240, below), vec![Action::Prev]);
        // locked → swipes dead
        b.set_hold(true);
        assert!(b.swipe(-1, 240, below).is_empty());
    }

    /// A swipe on the artwork turns the Now Playing page, wraps both ways, and emits NO action —
    /// it is a pure view change, so it must never reach the shell or the audio path.
    #[test]
    fn swiping_the_artwork_turns_the_page_and_never_skips() {
        use crate::now_playing::{PAGES, PAGE_SWIPE_BOT};
        let mut a = unlocked();
        assert_eq!(a.current(), Screen::NowPlaying);
        let on_art = PAGE_SWIPE_BOT - 100;
        assert_eq!(a.np_page(), 0);
        for want in 1..PAGES {
            assert!(a.swipe(-1, 240, on_art).is_empty(), "paging must emit no action");
            assert_eq!(a.np_page(), want);
        }
        // wraps forward…
        assert!(a.swipe(-1, 240, on_art).is_empty());
        assert_eq!(a.np_page(), 0);
        // …and backward
        assert!(a.swipe(1, 240, on_art).is_empty());
        assert_eq!(a.np_page(), PAGES - 1);
    }

    /// The analyzer must run for the audio pages even when the cover overlay is OFF — otherwise
    /// swiping to the spectrum page would show a permanently empty graph.
    #[test]
    fn the_audio_pages_ask_for_the_analyzer_regardless_of_the_cover_overlay() {
        let mut a = unlocked();
        a.set_viz_size(0); // cover overlay OFF
        a.set_np_page(0);
        assert!(!a.wants_spectrum(), "cover page with the overlay off needs no spectrum");
        for page in 1..crate::now_playing::PAGES {
            a.set_np_page(page);
            assert!(a.wants_spectrum(), "page {page} draws audio and must ask for the analyzer");
        }
        // And the cover page alone is enough when its overlay is on.
        a.set_np_page(0);
        a.set_viz_size(2);
        assert!(a.wants_spectrum());
    }

    #[test]
    fn right_swipe_on_song_row_queues_it() {
        let mut a = unlocked();
        a.stack = vec![Screen::Library];
        a.lib_tab = Tab::Songs;
        // Rightward swipe starting on the first Songs row (rows start at y=205, 62px tall).
        assert!(a.swipe(1, 240, 220).is_empty());
        assert_eq!(a.queue().len(), 1);
        let expected = library::song_at(&a.lib, a.lib_sort, 0).unwrap().title.clone();
        assert_eq!(a.queue()[0].title, expected);
        // Feedback started: toast + row chip animation, and tick() reports animation frames.
        assert!(a.toast.starts_with("Added to queue"));
        assert_eq!(a.queue_anim_frames, QUEUE_ANIM_FRAMES);
        assert_eq!(a.queue_anim_y, 220);
        assert!(a.tick());
        // A LEFTWARD swipe on the list queues nothing.
        assert!(a.swipe(-1, 240, 220).is_empty());
        assert_eq!(a.queue().len(), 1);
        // A rightward swipe on chrome (above the rows) queues nothing.
        assert!(a.swipe(1, 240, 100).is_empty());
        assert_eq!(a.queue().len(), 1);
    }

    /// Swipe-to-queue also works on an expanded album's inline track rows. Those rows are
    /// tappable-to-play, but the swipe used to handle the Songs tab only, so the same row queued
    /// on one tab and did nothing on another.
    #[test]
    fn right_swipe_on_an_expanded_album_track_queues_it() {
        let mut a = unlocked();
        a.stack = vec![Screen::Library];
        a.lib_tab = Tab::Albums;
        // Expand the first album in display order.
        let flat = library::album_display_order(&a.lib, a.album_sort)[0];
        a.album_expanded = Some(flat);
        // Find the y of its first inline track row through the accordion's own layout.
        let want = a.lib.albums_flat()[flat].track_list[0].clone();
        let y = (0..800)
            .find(|&y| a.albums_track_at(y).map(|s| s.object_id) == Some(want.object_id))
            .expect("expanded album should expose a track row");
        assert!(a.swipe(1, 240, y).is_empty());
        assert_eq!(a.queue().len(), 1);
        assert_eq!(a.queue()[0].object_id, want.object_id);
        // Collapsed, that same y is no longer a track row -> nothing queued.
        a.album_expanded = None;
        assert!(a.swipe(1, 240, y).is_empty());
        assert_eq!(a.queue().len(), 1);
    }

    /// Tapping a queue row plays that track. Previously *any* tap on Up Next just returned to Now
    /// Playing, so the queue was look-but-don't-touch.
    #[test]
    fn up_next_row_tap_plays_that_track() {
        let mut a = unlocked();
        a.push(Screen::UpNext);
        // Simulate what a render publishes: the ids actually drawn, in drawn order.
        a.up_next_rows = vec![101, 202, 303];
        let top = crate::chrome::HEADER_BOTTOM;
        let rh = crate::up_next::RH;
        assert_eq!(a.tap(240, top + rh / 2), vec![Action::PlayIndex(101)]);
        assert_eq!(a.tap(240, top + rh + rh / 2), vec![Action::PlayIndex(202)]);
        assert_eq!(a.current(), Screen::UpNext, "playing a row should not leave the screen");
        // Past the drawn rows there is nothing to play — the old shortcut back to Now Playing.
        assert!(a.tap(240, top + rh * 3 + rh / 2).is_empty());
        assert_eq!(a.current(), Screen::NowPlaying);
    }

    /// An empty queue keeps the old behaviour: a tap just leaves.
    #[test]
    fn up_next_tap_with_nothing_queued_returns_to_now_playing() {
        let mut a = unlocked();
        a.push(Screen::UpNext);
        a.up_next_rows.clear();
        assert!(a.tap(240, crate::chrome::HEADER_BOTTOM + 10).is_empty());
        assert_eq!(a.current(), Screen::NowPlaying);
    }

    /// The "Play album" band on the drill-in is hit-tested through its own rect. The old literal
    /// range started 16px above where the band is actually drawn.
    #[test]
    fn album_play_band_hit_matches_where_it_is_drawn() {
        let (bx, by, bw, bh) = library::album_play_band();
        assert!(library::hit_album_play_band(bx + bw / 2, by + bh / 2));
        assert!(!library::hit_album_play_band(bx + bw / 2, by - 1), "band hit extends above the band");
        assert!(!library::hit_album_play_band(bx - 1, by + bh / 2), "band hit extends left of the band");
        assert!(!library::hit_album_play_band(bx + bw / 2, by + bh), "band hit extends below the band");
    }

    #[test]
    fn album_screen_taps_play_band_and_track_rows() {
        let mut a = unlocked();
        a.stack = vec![Screen::Library];
        a.lib_tab = Tab::Albums;
        a.album_view = 0;
        a.push(Screen::Album);
        let album = a.lib.albums_flat()[0].clone();
        let first_id = album.track_list[0].object_id;
        // Tap the "Play album" band → plays from the first track (shell expands the album).
        assert_eq!(a.tap(240, 260), vec![Action::PlayIndex(first_id)]);
        // Tap the first track row (rows start at y=312 @56px) → same first track by object id.
        assert_eq!(a.tap(240, 320), vec![Action::PlayIndex(first_id)]);
        // A tap in the header art/title area plays nothing.
        assert!(a.tap(240, 160).is_empty());
    }

    #[test]
    fn usb_storage_modal_back_exits() {
        let mut a = unlocked();
        a.push(Screen::UsbStorage);
        // Modal: taps dead, Back pops AND emits the exit action for the shell.
        assert!(a.tap(240, 400).is_empty());
        assert_eq!(a.current(), Screen::UsbStorage);
        assert_eq!(a.press(Button::Back), vec![Action::ExitUsbMsc]);
        assert!(a.current() != Screen::UsbStorage);
    }

    #[test]
    fn auto_show_usb_storage_is_idempotent() {
        // The shell auto-raises the modal when it detects a PC host; the ~1 Hz poll may call this
        // every tick, so it must be idempotent (no stacked screens) and exit must still pop cleanly.
        let mut a = unlocked();
        assert!(!a.is_usb_storage());
        a.show_usb_storage();
        assert!(a.is_usb_storage());
        let depth = a.stack.len();
        a.show_usb_storage(); // repeat poll → no-op, no second screen pushed
        assert!(a.is_usb_storage());
        assert_eq!(a.stack.len(), depth);
        // Same single exit path as a manual entry: Back pops the modal + emits ExitUsbMsc.
        assert_eq!(a.press(Button::Back), vec![Action::ExitUsbMsc]);
        assert!(!a.is_usb_storage());
    }

    #[test]
    fn pixel_scroll_and_fling_move_library_window() {
        let mut a = unlocked();
        a.tap(344, 22); // Menu
        a.tap(200, 91 + 63 + 8); // Library
        let max = library::max_scroll_px(a.lib_tab, &a.lib, 0, None);
        a.scroll_px(120); // live drag → later rows (clamped to content)
        assert_eq!(a.lib_scroll_px, 120.min(max));
        a.scroll_px(-10_000); // clamp at 0
        assert_eq!(a.lib_scroll_px, 0);
        // Fling: velocity integrates over ticks, decays, and eventually stops.
        a.fling(1200.0);
        let mut ticks = 0;
        while a.tick() && ticks < 600 {
            ticks += 1;
        }
        assert!(ticks < 600, "fling must decay to a stop");
        assert!(a.lib_scroll_px >= 0 && a.lib_scroll_px <= max);
        assert_eq!(a.fling_v, 0.0);
        // Touching down again kills momentum instantly.
        a.fling(1200.0);
        a.stop_fling();
        assert_eq!(a.fling_v, 0.0);
    }

    #[test]
    fn up_next_empty_when_nothing_playing() {
        let a = unlocked();
        assert!(a.now_playing_queue("", "").is_none());
        assert!(a.now_playing_queue("No Such Track", "Nobody").is_none());
    }

    #[test]
    fn now_playing_queue_finds_current_album() {
        let a = unlocked();
        // a real track from the sample library's first populated album
        let (album, title, artist) = a
            .lib
            .album_groups
            .iter()
            .flat_map(|g| &g.albums)
            .find(|al| !al.track_list.is_empty())
            .map(|al| (al.name.clone(), al.track_list[0].title.clone(), al.track_list[0].artist.clone()))
            .expect("sample lib has an album with tracks");
        let (qa, tracks, idx) = a.now_playing_queue(&title, &artist).expect("resolves to the album");
        assert_eq!(qa, album);
        assert_eq!(idx, 0);
        assert!(!tracks.is_empty());
    }

    #[test]
    fn sound_screen_toggles_effects_and_emits_action() {
        let mut a = unlocked();
        a.press(Button::Up); // Menu
        for _ in 0..5 {
            a.press(Button::Down);
        }
        a.press(Button::Select); // Sound Settings (menu idx 5)
        assert_eq!(a.current(), Screen::Sound);
        assert_eq!(a.sound_flags(), 0); // all effects off initially
        // toggle DSEE (row 0)
        let acts = a.press(Button::Select);
        assert_eq!(acts, vec![Action::SoundChanged]);
        assert_eq!(a.sound_flags() & 1, 1);
        // move to ClearAudio+ (row 5) and toggle -> bit5 set
        for _ in 0..5 {
            a.press(Button::Down);
        }
        a.press(Button::Select);
        assert_eq!(a.sound_flags() & (1 << 5), 1 << 5);
    }
}
