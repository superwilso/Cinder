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
    /// SEEK within the current track, as permille (0..1000) of its duration. Emitted by the
    /// Now Playing progress rail (tap or drag). cinder-ffi turns it into ms using the DB duration.
    /// This is the rewind path the player was missing entirely — see `now_playing::hit_progress`.
    Seek(u16),
    /// Play the USER queue (swipe-to-queue) starting at index `n`. The shell resolves the whole
    /// queue to a track sequence, so ◁/▷ then step through what Up Next is actually showing.
    PlayQueueAt(usize),
    /// The UI text scale changed (Settings ▸ UI scale). Internal + persisted; no device call.
    UiScaleChanged,
}

/// The Menu rows, in display order — index ↔ destination Screen. Matches the prototype's 10 rows;
/// the Shelf is NOT here by design (it's opened from the status-bar bookmark glyph). "Help &
/// Controls" is the one Cinder addition (the onboarding intro, re-openable).
const MENU: [(Screen, &str, &str, &str); 11] = [
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
    (Screen::Onboarding, "note", "Help & Controls", "Button map · features"),
];

/// A pinned place on the Shelf: enough route context to jump straight back — and that means the
/// *whole* place, not just the screen. It used to store only `screen`/`lib_tab`/`album_view`, so
/// "jump back to where I was" dropped you at the top of the list with the wrong sort and no
/// accordion, which is most of why the Shelf felt broken. Pins now persist across boots too
/// (serialised into cinder_settings.conf by the shell).
#[derive(Clone, PartialEq, Debug)]
pub struct ShelfPin {
    screen: Screen,
    lib_tab: Tab,
    lib_sort: usize,
    album_sort: usize,
    album_expanded: Option<usize>,
    lib_scroll_px: i32,
    album_view: usize,
    album_scroll_px: i32,
    title: String,
    sub: String,
}

/// Screens a Shelf pin may point at. Anything else (modals, onboarding, the lock screen) is not a
/// "place" — restoring one would drop the user into a mode they never asked for.
fn pinnable(s: Screen) -> bool {
    matches!(
        s,
        Screen::NowPlaying
            | Screen::Library
            | Screen::Album
            | Screen::UpNext
            | Screen::Eq
            | Screen::Sound
            | Screen::Bluetooth
            | Screen::Settings
            | Screen::Fm
            | Screen::UsbDac
            | Screen::Receiver
    )
}

/// A short stable token for a Screen, for persisting Shelf pins across boots.
fn screen_token(s: Screen) -> &'static str {
    match s {
        Screen::NowPlaying => "np",
        Screen::Library => "lib",
        Screen::Album => "album",
        Screen::UpNext => "queue",
        Screen::Eq => "eq",
        Screen::Sound => "sound",
        Screen::Bluetooth => "bt",
        Screen::Settings => "settings",
        Screen::Fm => "fm",
        Screen::UsbDac => "usbdac",
        Screen::Receiver => "rx",
        _ => "np",
    }
}

fn screen_from_token(t: &str) -> Option<Screen> {
    Some(match t {
        "np" => Screen::NowPlaying,
        "lib" => Screen::Library,
        "album" => Screen::Album,
        "queue" => Screen::UpNext,
        "eq" => Screen::Eq,
        "sound" => Screen::Sound,
        "bt" => Screen::Bluetooth,
        "settings" => Screen::Settings,
        "fm" => Screen::Fm,
        "usbdac" => Screen::UsbDac,
        "rx" => Screen::Receiver,
        _ => return None,
    })
}

fn tab_token(t: Tab) -> &'static str {
    match t {
        Tab::Songs => "songs",
        Tab::Albums => "albums",
        Tab::Artists => "artists",
        Tab::Playlists => "playlists",
    }
}

fn tab_from_token(t: &str) -> Tab {
    match t {
        "songs" => Tab::Songs,
        "artists" => Tab::Artists,
        "playlists" => Tab::Playlists,
        _ => Tab::Albums,
    }
}

/// What the user's finger is currently dragging along a horizontal control. The shell asks
/// `scrub_begin` on every touch-down; a `true` answer routes the whole gesture here instead of
/// through the tap/swipe/vertical-drag classifier.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Scrub {
    None,
    /// Now Playing progress rail → seek (the rewind control).
    Progress,
    /// Settings ▸ UI scale slider.
    UiScale,
}

pub struct App {
    stack: Vec<Screen>,
    pub night: bool,
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
    /// The REAL link state, pushed by the shell (`cinder_set_bt_status`): -1 unknown (no detector
    /// on this firmware), 0 disconnected, 1 connected. The Bluetooth screen used to invent a
    /// connected "WH-1000XM5" whenever the UI toggle was on — it claimed a peer that may not
    /// exist, and gave no way to see that a headphone had dropped. An empty `bt_device` with
    /// `bt_link == 1` means "connected, name unresolved" (address-less link).
    bt_link: i8,
    bt_device: String,
    /// Device-wide BT transmit codec preference + LDAC quality tier. Used for BOTH normal BT
    /// playback and the USB-DAC→LDAC bridge. `bt_codec` indexes bluetooth::CODECS (0 = LDAC);
    /// `bt_ldac_quality` indexes bluetooth::QUALITIES (0 = Auto). Persisted; applied by the shell.
    bt_codec: u8,
    bt_ldac_quality: u8,
    /// USB-DAC mode engaged (input from a USB host → 3.5mm + BT/LDAC). Transient (not persisted).
    usb_dac_on: bool,
    /// Now Playing visualiser type (cinder_ui::viz index) + animation on/off (UI settings).
    viz_kind: u8,
    viz_on: bool,
    /// Settings screen cursor + its pixel scroll (the list is 828px tall on an 800px panel —
    /// without scrolling the whole ABOUT section was drawn off the bottom edge).
    settings_sel: usize,
    settings_scroll_px: i32,
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
    /// Active horizontal scrub (progress rail / UI-scale slider) + the rail's LIVE preview
    /// position while the finger is down, so the bar tracks the finger before the seek commits.
    scrub: Scrub,
    scrub_permille: u16,
    /// What the Up Next screen last DREW: the list index + track object_id of each visible row.
    /// `tap` resolves through this, so a tap can never land on a different row than the one under
    /// the finger — the same render-mirrors-hit rule the library lists follow. (`render` takes
    /// `&self`, hence the interior mutability.)
    up_next_rows: std::cell::RefCell<Vec<(usize, i64)>>,
    /// Is the drawn Up Next list the USER queue (true) or the current-album window (false)?
    up_next_is_queue: std::cell::Cell<bool>,
    /// The library tab strip as last DRAWN: (tab, x, width). Taps resolve through this, so the
    /// zones always match the labels on screen. They used to be hardcoded x thresholds that did
    /// NOT match the measured layout — tapping the left part of "ALBUMS" selected Songs, and so on
    /// down the strip (see `library::tab_layout`).
    lib_tab_zones: std::cell::RefCell<Vec<(Tab, f32, f32)>>,
}

/// How long the toast stays up (~1.8 s at the 60 fps pump).
const TOAST_FRAMES: u8 = 110;
/// Queue-chip slide animation length (~0.4 s at 60 fps).
const QUEUE_ANIM_FRAMES: u8 = 24;

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
            bt_link: -1, // unknown until the shell reports
            bt_device: String::new(),
            bt_codec: 0,        // LDAC
            bt_ldac_quality: 0, // Auto
            usb_dac_on: false,
            viz_kind: 0,
            viz_on: true,
            settings_sel: 0,
            settings_scroll_px: 0,
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
            scrub: Scrub::None,
            scrub_permille: 0,
            up_next_rows: std::cell::RefCell::new(Vec::new()),
            up_next_is_queue: std::cell::Cell::new(false),
            lib_tab_zones: std::cell::RefCell::new(Vec::new()),
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

    /// Capture the current place as a pin (full route context, so restoring it actually lands
    /// where you were — same tab, same sort, same scroll, same open accordion).
    fn capture_pin(&self) -> ShelfPin {
        let (title, sub) = self.place_label();
        ShelfPin {
            screen: self.current(),
            lib_tab: self.lib_tab,
            lib_sort: self.lib_sort,
            album_sort: self.album_sort,
            album_expanded: self.album_expanded,
            lib_scroll_px: self.lib_scroll_px,
            album_view: self.album_view,
            album_scroll_px: self.album_scroll_px,
            title,
            sub,
        }
    }

    /// Restore a pinned place. Two things this deliberately does NOT do any more:
    /// it no longer calls `go()` (which replaced the whole route stack, stranding the user with a
    /// dead Back button), and it no longer restores only the screen — the list position, sort and
    /// expansion come back too. Back from a restored pin returns to Now Playing.
    fn restore_pin(&mut self, p: &ShelfPin) {
        self.lib_tab = p.lib_tab;
        self.lib_sort = p.lib_sort.min(library::SORTS.len().saturating_sub(1));
        self.album_sort = p.album_sort.min(library::ALBUM_SORTS.len().saturating_sub(1));
        self.album_expanded = p.album_expanded;
        self.album_view = p.album_view;
        self.lib_idx = 0;
        self.fling_v = 0.0;
        self.stack = if p.screen == Screen::NowPlaying {
            vec![Screen::NowPlaying]
        } else {
            vec![Screen::NowPlaying, p.screen]
        };
        // Clamp the restored scroll against the CURRENT library (it may have been rebuilt since).
        self.lib_scroll_px = p.lib_scroll_px.clamp(0, self.lib_max_scroll());
        self.album_scroll_px = self
            .lib
            .albums_flat()
            .get(self.album_view)
            .map(|al| p.album_scroll_px.clamp(0, library::album_max_scroll_px(al)))
            .unwrap_or(0);
        self.settings_scroll_px = self.settings_scroll_px.clamp(0, crate::settings::max_scroll_px());
    }

    /// Handle a tap while the Shelf overlay is open (geometry comes from `shelf::hit`).
    fn shelf_tap(&mut self, x: i32, y: i32) -> Vec<Action> {
        use crate::shelf::ShelfHit;
        let filled = [self.pins[0].is_some(), self.pins[1].is_some(), self.pins[2].is_some()];
        match crate::shelf::hit(x, y, filled) {
            ShelfHit::Close => self.shelf_open = false,
            ShelfHit::Back => {
                self.shelf_open = false;
                self.pop();
            }
            ShelfHit::PinTo(i) => {
                if !pinnable(self.current()) {
                    self.notify("Nothing to pin here");
                } else {
                    let replaced = self.pins[i].is_some();
                    self.pins[i] = Some(self.capture_pin());
                    let name = self.pins[i].as_ref().map(|p| p.title.clone()).unwrap_or_default();
                    // Say what happened AND which slot — the old silent "first empty, else
                    // clobber slot 0" left the user guessing.
                    self.notify(&if replaced {
                        format!("Slot {} replaced — {}", i + 1, name)
                    } else {
                        format!("Pinned to slot {} — {}", i + 1, name)
                    });
                }
            }
            ShelfHit::Go(i) => {
                if let Some(p) = self.pins.get(i).and_then(|p| p.clone()) {
                    self.restore_pin(&p);
                }
                self.shelf_open = false;
            }
            ShelfHit::Clear(i) => {
                if let Some(slot) = self.pins.get_mut(i) {
                    if slot.take().is_some() {
                        self.notify(&format!("Slot {} cleared", i + 1));
                    }
                }
            }
            ShelfHit::None => {}
        }
        vec![]
    }

    /// Pop a transient toast (also used for shelf feedback and queue confirmations).
    fn notify(&mut self, msg: &str) {
        self.toast = msg.to_string();
        self.toast_frames = TOAST_FRAMES;
    }

    // ── Shelf pin persistence ───────────────────────────────────────────────────────────────
    // Pins were session-scoped, so every reboot wiped the user's bookmarks. They serialise to one
    // line per slot; the shell stores them in cinder_settings.conf alongside the rest.

    /// Serialise slot `i` as `screen|tab|lib_sort|album_sort|expanded|lib_scroll|album|album_scroll|title|sub`.
    /// Empty string = the slot is empty. `|` is stripped from the labels so a title can't corrupt the record.
    pub fn shelf_pin_encode(&self, i: usize) -> String {
        let Some(p) = self.pins.get(i).and_then(|p| p.as_ref()) else { return String::new() };
        let clean = |s: &str| s.replace(['|', '\n'], " ");
        format!(
            "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
            screen_token(p.screen),
            tab_token(p.lib_tab),
            p.lib_sort,
            p.album_sort,
            p.album_expanded.map(|e| e as i64).unwrap_or(-1),
            p.lib_scroll_px,
            p.album_view,
            p.album_scroll_px,
            clean(&p.title),
            clean(&p.sub),
        )
    }

    /// Restore slot `i` from `shelf_pin_encode` output. A malformed/empty record clears the slot
    /// rather than failing — a corrupt config must never keep the player from booting.
    pub fn shelf_pin_decode(&mut self, i: usize, s: &str) {
        if i >= self.pins.len() {
            return;
        }
        let f: Vec<&str> = s.split('|').collect();
        if f.len() < 10 {
            self.pins[i] = None;
            return;
        }
        let Some(screen) = screen_from_token(f[0]) else {
            self.pins[i] = None;
            return;
        };
        let num = |s: &str| s.trim().parse::<i64>().unwrap_or(0);
        let exp = num(f[4]);
        self.pins[i] = Some(ShelfPin {
            screen,
            lib_tab: tab_from_token(f[1]),
            lib_sort: (num(f[2]).max(0) as usize).min(library::SORTS.len() - 1),
            album_sort: (num(f[3]).max(0) as usize).min(library::ALBUM_SORTS.len() - 1),
            album_expanded: if exp < 0 { None } else { Some(exp as usize) },
            lib_scroll_px: num(f[5]).max(0) as i32,
            album_view: num(f[6]).max(0) as usize,
            album_scroll_px: num(f[7]).max(0) as i32,
            title: f[8].to_string(),
            sub: f[9].to_string(),
        });
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
            self.stack.push(s);
        }
    }
    fn pop(&mut self) {
        if self.stack.len() > 1 {
            self.stack.pop();
        }
    }

    /// Step the UI text scale by `d` stops (Left/Right on the Settings slider row).
    fn step_ui_scale(&mut self, d: i32) -> Vec<Action> {
        let n = crate::text::SCALE_STEPS.len() as i32;
        let idx = (crate::text::scale_idx() as i32 + d).clamp(0, n - 1) as usize;
        crate::text::set_scale_idx(idx);
        vec![Action::UiScaleChanged]
    }

    /// Set the UI text scale from a slider stop index.
    fn set_ui_scale_idx(&mut self, idx: usize) -> Vec<Action> {
        crate::text::set_scale_idx(idx);
        vec![Action::UiScaleChanged]
    }

    /// The UI text scale in percent — the shell persists this and restores it at boot.
    pub fn ui_scale_pct(&self) -> u32 {
        crate::text::scale_pct()
    }
    pub fn set_ui_scale_pct(&mut self, pct: u32) {
        crate::text::set_scale_pct(pct);
    }

    // Activate the focused Settings row (shared by the Select button and a tap).
    fn settings_activate(&mut self) -> Vec<Action> {
        match self.settings_sel {
            crate::settings::ROW_UI_SCALE => self.step_ui_scale(1),
            crate::settings::ROW_THEME => {
                self.night = !self.night;
                vec![Action::ThemeChanged(self.night)]
            }
            crate::settings::ROW_VIZ => {
                self.cycle_viz();
                vec![]
            }
            crate::settings::ROW_VIZ_ANIM => {
                self.viz_on = !self.viz_on;
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
        // Global chrome (status bar, the full top strip). The bookmark glyph (top-right, drawn at
        // x≈392) opens the **Shelf**; tapping anywhere else on the bar opens the **Menu** — the rest
        // of the strip stays one big forgiving Menu target. Header back chevron → Back (below).
        if y < 34 {
            if (380..=406).contains(&x) {
                self.open_shelf();
            } else if self.current() != Screen::Menu {
                self.push(Screen::Menu);
            }
            return vec![];
        }
        // Back chevron: a generous ≥44px target (the whole header-left block, from just under
        // the status strip to the header rule) on every screen that draws one.
        let has_header = !matches!(self.current(), Screen::NowPlaying | Screen::Menu | Screen::Lock);
        if has_header && (34..91).contains(&y) && x < 80 {
            self.pop();
            return vec![];
        }

        match self.current() {
            Screen::Menu => {
                if y >= 91 {
                    let row = ((y - 91) / 63) as usize;
                    if row < MENU.len() {
                        self.menu_idx = row;
                        self.activate_menu(row);
                    }
                }
                vec![]
            }
            Screen::NowPlaying => {
                let hit = |cx: i32, cy: i32, r: i32| (x - cx).pow(2) + (y - cy).pow(2) <= r * r;
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
                    // The "Play album" band (shuffle_row @234..306): play from track 1 — the
                    // shell's album_context expands it to the whole album in order.
                    if (234..306).contains(&y) {
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
                // Tapping a row PLAYS it. Previously the whole screen was one "go back to Now
                // Playing" target, so the queue was a dead list you could look at but not use.
                // `up_next_rows` is what the last render actually drew, so the tap can't drift.
                let hit = crate::up_next::drawn_row_at(y)
                    .and_then(|d| self.up_next_rows.borrow().get(d).copied());
                match hit {
                    Some((idx, _)) if self.up_next_is_queue.get() => vec![Action::PlayQueueAt(idx)],
                    Some((_, object_id)) => vec![Action::PlayIndex(object_id)],
                    // Empty area below the list: the old "done with the queue" gesture, but via
                    // pop() so the route stack (and therefore Back) survives.
                    None => {
                        self.pop();
                        vec![]
                    }
                }
            }
            Screen::Settings => {
                if let Some(row) = crate::settings::row_at(y, self.settings_scroll_px) {
                    self.settings_sel = row;
                    // The UI-scale row is a SLIDER: the tap's x picks the stop directly (the
                    // Android SeekBar idiom) rather than cycling one step per tap.
                    if row == crate::settings::ROW_UI_SCALE {
                        let idx = crate::settings::ui_scale_idx_at(x);
                        return self.set_ui_scale_idx(idx);
                    }
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
                if y >= 91 {
                    let row = ((y - 91) / 64) as usize;
                    if row < crate::sound::ROWS {
                        self.sound_sel = row;
                        return self.sound_toggle_row();
                    }
                }
                vec![]
            }
            Screen::Eq => {
                // preset pills row (py = y0+6 = 97, ~30px tall) — even split across the 5 presets
                if (92..130).contains(&y) {
                    let idx = (((x - 22).max(0)) / 86).min((data::EQ_PRESETS.len() - 1) as i32) as usize;
                    self.eq_preset = idx;
                    self.eq_bands = data::EQ_PRESETS[idx].1;
                    return vec![Action::EqChanged(self.eq_bands)];
                }
                // band field: tap a column to select it; tap above mid raises, below lowers
                if y >= 150 && y < 600 {
                    let band = (((x - 22).max(0)) / 44).min(9) as usize;
                    self.eq_sel = band;
                    let g = &mut self.eq_bands[band];
                    if y < 375 {
                        *g = (*g + 1).min(6);
                    } else {
                        *g = (*g - 1).max(-6);
                    }
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
                // single-purpose screen: any tap toggles USB-DAC (→ 3.5mm + BT/LDAC). Mass storage
                // lives on the Settings "USB mode" row, not here.
                self.usb_dac_on = !self.usb_dac_on;
                vec![Action::UsbDacToggle(self.usb_dac_on)]
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
        if (library::TAB_TOP..library::TAB_BOT).contains(&y) {
            let Some(tab) = self.tab_at_cached(x) else { return vec![] };
            self.lib_tab = tab;
            self.lib_idx = 0;
            self.lib_scroll_px = 0;
            self.fling_v = 0.0;
            self.album_expanded = None;
            return vec![];
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
            // Artists / Playlists rows aren't directly playable (no track object under the finger) —
            // they navigate, not play. Nothing to enqueue.
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

    /// Which library tab is at screen-x `x`, using the strip as it was last drawn? Splits the
    /// gaps between labels down the middle. Falls back to an even quarter split if nothing has
    /// been rendered yet (a tap before the first paint isn't reachable in practice).
    fn tab_at_cached(&self, x: i32) -> Option<Tab> {
        let zones = self.lib_tab_zones.borrow();
        if zones.is_empty() {
            return Some(match x * 4 / crate::canvas::W as i32 {
                0 => Tab::Songs,
                1 => Tab::Albums,
                2 => Tab::Artists,
                _ => Tab::Playlists,
            });
        }
        let xf = x as f32;
        for (i, &(tab, tx, tw)) in zones.iter().enumerate() {
            let left = if i == 0 {
                0.0
            } else {
                let (_, px, pw) = zones[i - 1];
                (px + pw + tx) / 2.0
            };
            let right = match zones.get(i + 1) {
                Some(&(_, nx, _)) => (tx + tw + nx) / 2.0,
                None => crate::canvas::W as f32,
            };
            if xf >= left && xf < right {
                return Some(tab);
            }
        }
        None
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
        // The Shelf is a MODAL bottom sheet: it owns the gesture. Without this guard a drag that
        // started on the sheet scrolled (and flung) the library list behind it, which read as the
        // Shelf "not working" — the sheet sat still while the screen moved underneath it.
        if self.shelf_open || self.locked {
            return;
        }
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
        if self.shelf_open || self.locked {
            return;
        }
        if matches!(self.current(), Screen::Library | Screen::Album | Screen::Settings) {
            self.fling_v = velocity_px_s.clamp(-8000.0, 8000.0);
        }
    }

    // ── Horizontal scrub (progress rail / UI-scale slider) ──────────────────────────────────
    // The shell offers every touch-down here first. Answering `true` claims the whole gesture:
    // motion streams to `scrub_move`, release commits in `scrub_end`, and the tap/swipe/vertical
    // -drag classifier never sees it. That is what makes the two horizontal controls in the UI
    // (seek and UI scale) behave like real sliders rather than tap-only targets.

    /// A finger went down at (x, y). Returns true if it grabbed a scrubbable control.
    pub fn scrub_begin(&mut self, x: i32, y: i32) -> bool {
        self.scrub = Scrub::None;
        if self.locked || self.shelf_open {
            return false;
        }
        match self.current() {
            Screen::NowPlaying => match crate::now_playing::hit_progress(x, y) {
                Some(p) => {
                    self.scrub = Scrub::Progress;
                    self.scrub_permille = p;
                    true
                }
                None => false,
            },
            Screen::Settings => {
                if crate::settings::row_at(y, self.settings_scroll_px) == Some(crate::settings::ROW_UI_SCALE) {
                    self.scrub = Scrub::UiScale;
                    self.settings_sel = crate::settings::ROW_UI_SCALE;
                    crate::text::set_scale_idx(crate::settings::ui_scale_idx_at(x));
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    /// The finger moved to `x` during a scrub. The progress rail only PREVIEWS here (committing a
    /// seek per motion event would hammer PlayerService); the UI-scale slider applies live.
    pub fn scrub_move(&mut self, x: i32) -> Vec<Action> {
        match self.scrub {
            Scrub::Progress => {
                self.scrub_permille = crate::now_playing::permille_at(x);
                vec![]
            }
            Scrub::UiScale => {
                crate::text::set_scale_idx(crate::settings::ui_scale_idx_at(x));
                vec![]
            }
            Scrub::None => vec![],
        }
    }

    /// The finger lifted: commit the scrub.
    pub fn scrub_end(&mut self) -> Vec<Action> {
        let acts = match self.scrub {
            Scrub::Progress => vec![Action::Seek(self.scrub_permille)],
            Scrub::UiScale => vec![Action::UiScaleChanged],
            Scrub::None => vec![],
        };
        self.scrub = Scrub::None;
        acts
    }

    /// Is a scrub in progress? (The shell keeps routing motion here while true.)
    pub fn is_scrubbing(&self) -> bool {
        self.scrub != Scrub::None
    }

    /// Keep the Settings cursor's row inside the scrolled window (button nav).
    fn settings_ensure_visible(&mut self) {
        let top = crate::settings::row_top_px(self.settings_sel);
        let rh = crate::settings::row_h();
        let view = crate::settings::LIST_BOTTOM - crate::settings::LIST_TOP;
        let max = crate::settings::max_scroll_px();
        if top < self.settings_scroll_px {
            self.settings_scroll_px = top;
        } else if top + rh > self.settings_scroll_px + view {
            self.settings_scroll_px = top + rh - view;
        }
        self.settings_scroll_px = self.settings_scroll_px.clamp(0, max);
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
                if dir < 0 {
                    vec![Action::Next]
                } else {
                    vec![Action::Prev]
                }
            }
            Screen::Library if dir > 0 => {
                // Right-swipe a Songs-tab row → queue that song (the same render-mirroring hit
                // test the tap uses, so the queued song is exactly the row under the finger).
                if self.lib_tab == Tab::Songs {
                    if let Some(rank) = library::hit_row(self.lib_tab, &self.lib, self.lib_scroll_px, y)
                    {
                        if let Some(s) = library::song_at(&self.lib, self.lib_sort, rank) {
                            let s = s.clone();
                            self.enqueue(s, y);
                        }
                    }
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
        self.notify(&format!("Added to queue — {}", s.title));
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

    /// Number of rows in the current library tab (for cursor clamping).
    fn lib_len(&self) -> usize {
        library::row_count(self.lib_tab, &self.lib)
    }

    /// Replace the browsable library (called by the shell after the real DB is read). Resets
    /// the cursor so a stale index can't point past the new contents.
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
                    // Artists/Playlists rows navigate, not play (no track under the cursor).
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
                    self.settings_ensure_visible();
                    vec![]
                }
                Button::Down => {
                    if self.settings_sel + 1 < crate::settings::ROWS {
                        self.settings_sel += 1;
                        self.settings_ensure_visible();
                    }
                    vec![]
                }
                // On the UI-scale slider, Left/Right step the value (Select still steps up).
                Button::Left if self.settings_sel == crate::settings::ROW_UI_SCALE => self.step_ui_scale(-1),
                Button::Right if self.settings_sel == crate::settings::ROW_UI_SCALE => self.step_ui_scale(1),
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
            // The remaining screens (Fm/Receiver/UpNext/Pairing): Back pops, everything else is a
            // no-op until their per-screen controls are wired.
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
        // Publish the LIVE status bar for every screen. This used to be per-screen literals, so
        // outside Now Playing / Lock the device showed a frozen "14:32 · FLAC 24/96 · 78%".
        crate::chrome::set_status(np.clock, np.badge, np.battery);
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
                // inject the selected visualiser type + on/off (UI state) into the now-playing
                // data; while the finger is on the progress rail the bar shows the LIVE scrub
                // position instead of playback's, so the seek has visible feedback before it lands.
                let scrubbing = self.scrub == Scrub::Progress;
                let np2 = NowPlaying {
                    viz_kind: self.viz_kind,
                    viz_on: self.viz_on,
                    scrubbing,
                    progress: if scrubbing { self.scrub_permille as f32 / 1000.0 } else { np.progress },
                    ..*np
                };
                crate::now_playing::render(c, &theme, fonts, &np2);
                // sleep-timer countdown badge (nav owns the live remaining minutes)
                crate::now_playing::sleep_badge(c, &theme, fonts, self.sleep_min);
            }
            Screen::Menu => {
                // The Library row's caption reflects the real library size.
                let lib_value = if self.lib.is_empty() {
                    String::from("Empty")
                } else {
                    format!("{} albums · {} tracks", self.lib.album_count(), self.lib.songs.len())
                };
                let queue_value = if self.queue.is_empty() {
                    String::from("Queue empty")
                } else {
                    format!("{} queued", self.queue.len())
                };
                let items: Vec<MenuItem> = MENU
                    .iter()
                    .enumerate()
                    .map(|(i, (screen, icon, label, value))| MenuItem {
                        icon,
                        label,
                        value: match *screen {
                            Screen::Library => &lib_value,
                            Screen::UpNext => &queue_value,
                            _ => value,
                        },
                        active: i == self.menu_idx,
                    })
                    .collect();
                crate::menu::render(c, &theme, fonts, &items);
            }
            Screen::Library => {
                // Record the tab strip exactly as drawn, so `tap` hits the labels the user sees.
                *self.lib_tab_zones.borrow_mut() = crate::library::tab_layout(fonts);
                crate::library::render(
                    c, &theme, fonts, self.lib_tab, self.lib_idx, self.lib_scroll_px, self.lib_sort,
                    self.album_sort, self.album_expanded, &self.lib,
                )
            }
            Screen::Album => {
                let flat = self.lib.albums_flat();
                if let Some(al) = flat.get(self.album_view) {
                    crate::library::album_view(
                        c, &theme, fonts, al, self.album_track_idx, self.album_scroll_px,
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
                // Record exactly which list rows land on which drawn rows, so `tap` resolves the
                // row under the finger rather than guessing (the window scroll depends on `np`,
                // which tap never sees).
                let mut drawn: Vec<(usize, i64)> = Vec::new();
                if !self.queue.is_empty() {
                    // The user's own queue (swipe-to-queue) takes precedence over the derived
                    // current-album list.
                    self.up_next_is_queue.set(true);
                    crate::up_next::render_queue(c, &theme, fonts, &self.queue);
                    for (i, s) in self.queue.iter().enumerate().take(crate::up_next::visible_rows()) {
                        drawn.push((i, s.object_id));
                    }
                } else {
                    self.up_next_is_queue.set(false);
                    match self.now_playing_queue(np.title, np.artist) {
                        Some((album, tracks, cur)) => {
                            crate::up_next::render(c, &theme, fonts, album, tracks, cur);
                            let scroll = crate::up_next::window_scroll(tracks.len(), cur);
                            for (i, s) in tracks.iter().enumerate().skip(scroll).take(crate::up_next::visible_rows()) {
                                drawn.push((i, s.object_id));
                            }
                        }
                        None => crate::up_next::render(c, &theme, fonts, "", &[], 0),
                    }
                }
                *self.up_next_rows.borrow_mut() = drawn;
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
                    bt_codec: self.bt_peer().map(|_| crate::bluetooth::CODECS[self.bt_codec as usize].0),
                };
                crate::sound::render(c, &theme, fonts, &snd, self.sound_sel, self.snd_ab_bypass)
            }
            Screen::Bluetooth => {
                let bt = Bt {
                    on: self.bt_on,
                    connected: self.bt_peer(),
                    link_known: self.bt_link_known(),
                    codec_sel: self.bt_codec,
                    ldac_quality: self.bt_ldac_quality,
                };
                crate::bluetooth::render(c, &theme, fonts, &bt)
            }
            Screen::Settings => {
                let sleep_lbl = self.sleep_label();
                let view = crate::settings::SettingsView {
                    night: self.night,
                    viz_name: crate::viz::name(self.viz_kind),
                    viz_on: self.viz_on,
                    usb_dac: self.usb_dac_on,
                    battery_care: self.battery_care,
                    storage: self.storage_label(),
                    sleep: &sleep_lbl,
                };
                crate::settings::render(c, &theme, fonts, self.settings_sel, self.settings_scroll_px, &view)
            }
            Screen::Fm => crate::fm::render(c, &theme, fonts, 88.6),
            Screen::UsbDac => {
                let dev = self.bt_peer();
                let ldac = self.usb_dac_on && dev.is_some();
                let codec = crate::bluetooth::CODECS[self.bt_codec as usize].0;
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
        // Swipe-to-queue chip riding the flicked row (list screens only — if the user navigates
        // away mid-animation the anchor row is gone, so it just stops). It is anchored to a ROW,
        // so it belongs with the screen, under any overlay.
        if self.queue_anim_frames > 0 && matches!(self.current(), Screen::Library | Screen::Album) {
            let p = self.queue_anim_frames as f32 / QUEUE_ANIM_FRAMES as f32;
            crate::overlay::queue_chip(c, &theme, fonts, self.queue_anim_y, p);
        }
        // Shelf bottom-sheet: dims the screen behind + draws the sheet over the lower half.
        if self.shelf_open {
            let (title, sub) = self.place_label();
            let pins = [
                self.pins[0].as_ref().map(|p| crate::shelf::Pin { title: &p.title, sub: &p.sub }),
                self.pins[1].as_ref().map(|p| crate::shelf::Pin { title: &p.title, sub: &p.sub }),
                self.pins[2].as_ref().map(|p| crate::shelf::Pin { title: &p.title, sub: &p.sub }),
            ];
            crate::shelf::render(c, &theme, fonts, &title, &sub, &pins);
        }
        // TRANSIENTS LAST — above the Shelf sheet too. Drawn before it, the sheet (which fills
        // y 406..800 opaquely) painted straight over both: the pin/clear confirmation was
        // invisible in the one place it fires, and the volume HUD — which still responds to Vol±
        // while the sheet is open — came up half-covered.
        if self.vol_overlay > 0 && self.current() != Screen::Lock {
            crate::overlay::volume(c, &theme, fonts, self.volume);
        }
        if self.toast_frames > 0 && self.current() != Screen::Lock {
            crate::overlay::toast(c, &theme, fonts, &self.toast);
        }
    }

    /// Advance per-frame timers (overlay countdowns). The shell calls this once per pump tick
    /// before `render`. Returns true while something time-driven still needs redrawing.
    pub fn tick(&mut self) -> bool {
        // Return true for EVERY counting-down frame, INCLUDING the one that reaches 0 — that
        // frame must repaint (now without the HUD/toast) to clear it. (With dirty-flag rendering,
        // returning false on the 0-transition would leave it stuck on screen.)
        let mut animating = false;
        // Fling momentum: integrate at the ~60 fps tick, exponential decay, stop below a
        // threshold. Hitting the clamp (top/bottom) kills it immediately.
        if self.fling_v != 0.0 {
            let pos = |a: &Self| match a.current() {
                Screen::Library => a.lib_scroll_px,
                Screen::Album => a.album_scroll_px,
                Screen::Settings => a.settings_scroll_px,
                _ => 0,
            };
            let before = pos(self);
            let step = self.fling_v / 60.0;
            self.scroll_px(step as i32);
            let after = pos(self);
            self.fling_v *= 0.92;
            if self.fling_v.abs() < 30.0 || (step as i32 != 0 && after == before) {
                self.fling_v = 0.0;
            }
            animating = true;
        }
        if self.vol_overlay > 0 {
            self.vol_overlay -= 1;
            animating = true;
        }
        if self.toast_frames > 0 {
            self.toast_frames -= 1;
            animating = true;
        }
        if self.queue_anim_frames > 0 {
            self.queue_anim_frames -= 1;
            animating = true;
        }
        animating
    }

    /// Is anything time-driven still on screen? The shell uses this (via `cinder_needs_frame`)
    /// to decide between a full-rate frame and an idle sleep, so a still UI costs nothing.
    pub fn is_animating(&self) -> bool {
        self.fling_v != 0.0 || self.vol_overlay > 0 || self.toast_frames > 0 || self.queue_anim_frames > 0
    }

    /// Push the real Bluetooth link state from the shell: `state` < 0 = unknown (this firmware
    /// exposes no way to tell), 0 = disconnected, 1 = connected. Returns true if anything changed,
    /// so the caller only repaints when it must. A live link also implies the radio is on.
    pub fn set_bt_link(&mut self, state: i32, name: &str) -> bool {
        let st: i8 = if state < 0 { -1 } else if state > 0 { 1 } else { 0 };
        let changed = st != self.bt_link || name != self.bt_device;
        if changed {
            self.bt_link = st;
            self.bt_device = name.to_string();
            if st == 1 {
                self.bt_on = true;
            }
        }
        changed
    }

    /// Is a Bluetooth sink actually connected right now? (False when unknown.)
    pub fn bt_connected(&self) -> bool {
        self.bt_link == 1
    }

    /// Does the shell know the link state at all?
    pub fn bt_link_known(&self) -> bool {
        self.bt_link >= 0
    }

    /// The connected peer's label for the Bluetooth/USB-DAC screens, or None when nothing is
    /// linked (or we can't tell). Honest: no confirmed link → no card, whatever the toggle says.
    fn bt_peer(&self) -> Option<&str> {
        if !(self.bt_on && self.bt_link == 1) {
            return None;
        }
        Some(if self.bt_device.is_empty() { "Connected device" } else { self.bt_device.as_str() })
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
    pub fn viz_on(&self) -> bool {
        self.viz_on
    }
    pub fn set_viz_kind(&mut self, k: u8) {
        self.viz_kind = k % crate::viz::COUNT;
    }
    pub fn set_viz_on(&mut self, on: bool) {
        self.viz_on = on;
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

    /// The UI text scale is a process-wide global (there is exactly one UI per process on the
    /// device). `cargo test` runs tests on several threads, so every test that changes or depends
    /// on the scale takes this lock — otherwise one test's 140% leaks into another's measurements.
    static SCALE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Take the scale lock and restore 100% on the way out, even if the test panics.
    struct ScaleGuard(#[allow(dead_code)] std::sync::MutexGuard<'static, ()>);
    impl Drop for ScaleGuard {
        fn drop(&mut self) {
            crate::text::set_scale_pct(100);
        }
    }
    fn lock_scale() -> ScaleGuard {
        ScaleGuard(SCALE_LOCK.lock().unwrap_or_else(|e| e.into_inner()))
    }

    /// A minimal live now-playing payload for tests that need `render` to run.
    fn sample_np() -> NowPlaying<'static> {
        NowPlaying {
            title: "",
            artist: "",
            codec: "",
            badge: "",
            clock: "14:32",
            battery: 78,
            elapsed: "",
            remaining: "",
            progress: 0.0,
            art: "",
            art_full: None,
            art_thumb: None,
            liked: false,
            playing: true,
            shuffle: false,
            repeat: 0,
            viz_seed: 2.0,
            viz_kind: 0,
            viz_on: false,
            viz_levels: None,
            scrubbing: false,
        }
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
        a.tap(372, 16); // status bar (not the bookmark) → Menu
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
        a.tap(120, 16); // mid status bar → Menu
        assert_eq!(a.current(), Screen::Menu);
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
        // Left from the default Albums tab lands on Songs (Artists/Playlists rows navigate,
        // not play — only Songs rows emit PlayIndex directly).
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
        // cursor down to the Visualiser row and cycle it (index-agnostic — rows get inserted)
        for _ in 0..crate::settings::ROW_VIZ {
            a.press(Button::Down);
        }
        assert_eq!(a.settings_sel, crate::settings::ROW_VIZ);
        let k1 = a.viz_kind();
        a.press(Button::Select);
        assert_eq!(a.viz_kind(), (k1 + 1) % crate::viz::COUNT);
        // down to Visualiser anim (ROW_VIZ_ANIM=2) and toggle it
        a.press(Button::Down);
        assert_eq!(a.settings_sel, crate::settings::ROW_VIZ_ANIM);
        let on = a.viz_on();
        a.press(Button::Select);
        assert_eq!(a.viz_on(), !on);
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
        // a tap engages USB-DAC → routes to 3.5mm + BT/LDAC (no BT disconnect)
        assert_eq!(a.tap(240, 300), vec![Action::UsbDacToggle(true)]);
        assert!(a.usb_dac_on());
        // tapping again disengages
        assert_eq!(a.tap(240, 300), vec![Action::UsbDacToggle(false)]);
        assert!(!a.usb_dac_on());
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
        // status bar opens the Menu from anywhere along the top strip — not just the ☰ icon.
        a.tap(20, 16); // far-left (over the clock) used to be a dead zone
        assert_eq!(a.current(), Screen::Menu);
        a.tap(200, 91 + 63 + 8); // leave Menu (into Library) so the next assert is meaningful
        a.tap(372, 16); // right side (the ☰ icon) → Menu too
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
        // Now Playing: swipe = the same transport actions as the skip buttons
        let mut b = unlocked();
        assert_eq!(b.current(), Screen::NowPlaying);
        assert_eq!(b.swipe(-1, 240, 400), vec![Action::Next]);
        assert_eq!(b.swipe(1, 240, 400), vec![Action::Prev]);
        // locked → swipes dead
        b.set_hold(true);
        assert!(b.swipe(-1, 240, 400).is_empty());
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
        a.tap(372, 16); // Menu
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

    // ── Rewind / seek ───────────────────────────────────────────────────────────────────────

    #[test]
    fn progress_rail_is_a_scrub_target() {
        // The regression: the rail was drawn but was NOT a tap target anywhere, and no code path
        // in the whole player ever called seek. Rewinding within a track was impossible.
        let mut a = unlocked(); // Now Playing
        let y = crate::now_playing::RAIL_Y;
        assert!(a.scrub_begin(crate::now_playing::RAIL_X0, y), "rail must claim the gesture");
        assert!(a.is_scrubbing());
        // Drag to the middle, then release → one Seek at the released position.
        a.scrub_move(crate::now_playing::RAIL_X0 + crate::now_playing::RAIL_W / 2);
        let acts = a.scrub_end();
        assert!(matches!(acts.as_slice(), [Action::Seek(p)] if (480..=520).contains(p)), "{acts:?}");
        assert!(!a.is_scrubbing());
    }

    #[test]
    fn rail_scrub_is_generous_and_clamped() {
        let mut a = unlocked();
        // 44px-class band: a tap well above/below the 6px rail still grabs it.
        assert!(a.scrub_begin(240, crate::now_playing::RAIL_Y - 14));
        a.scrub_end();
        assert!(a.scrub_begin(240, crate::now_playing::RAIL_Y + 20));
        // Dragging past either end pins to 0 / 1000 instead of doing nothing.
        a.scrub_move(-500);
        assert_eq!(a.scrub_end(), vec![Action::Seek(0)]);
        assert!(a.scrub_begin(240, crate::now_playing::RAIL_Y));
        a.scrub_move(9999);
        assert_eq!(a.scrub_end(), vec![Action::Seek(1000)]);
        // Off the rail entirely → not a scrub (the transport row must still work).
        assert!(!a.scrub_begin(240, 692));
    }

    #[test]
    fn locked_or_shelf_open_never_scrubs() {
        let mut a = unlocked();
        a.open_shelf();
        assert!(!a.scrub_begin(240, crate::now_playing::RAIL_Y));
        let mut b = unlocked();
        b.set_hold(true);
        assert!(!b.scrub_begin(240, crate::now_playing::RAIL_Y));
    }

    // ── Up Next: the queue is playable ──────────────────────────────────────────────────────

    #[test]
    fn up_next_row_tap_plays_the_row_under_the_finger() {
        let mut a = unlocked();
        // Queue three songs (the Spotify-style right-swipe), then open Up Next.
        a.stack = vec![Screen::Library];
        a.lib_tab = Tab::Songs;
        for i in 0..3 {
            a.swipe(1, 240, 220 + i * library::row_h(Tab::Songs));
        }
        assert_eq!(a.queue().len(), 3);
        a.push(Screen::UpNext);
        // Render first — `tap` resolves through what was actually drawn.
        let fonts = FontSet::load();
        let mut c = Canvas::new();
        a.render(&mut c, &fonts, &NowPlaying { title: "", artist: "", ..sample_np() });
        // Row 1 of the queue → play the queue from index 1 (previously: the whole screen was one
        // "go back to Now Playing" target and the queue could not be played at all).
        let y = crate::up_next::LIST_TOP + crate::up_next::RH + 10;
        assert_eq!(a.tap(240, y), vec![Action::PlayQueueAt(1)]);
    }

    #[test]
    fn up_next_album_row_tap_plays_that_track() {
        let mut a = unlocked();
        a.push(Screen::UpNext);
        // A track that IS in the sample library, so now_playing_queue resolves its album.
        let (title, artist, ids) = a
            .lib
            .album_groups
            .iter()
            .flat_map(|g| &g.albums)
            .find(|al| al.track_list.len() > 2)
            .map(|al| {
                (
                    al.track_list[0].title.clone(),
                    al.track_list[0].artist.clone(),
                    al.track_list.iter().map(|s| s.object_id).collect::<Vec<_>>(),
                )
            })
            .expect("sample album");
        let fonts = FontSet::load();
        let mut c = Canvas::new();
        a.render(&mut c, &fonts, &NowPlaying { title: &title, artist: &artist, ..sample_np() });
        let y = crate::up_next::LIST_TOP + 2 * crate::up_next::RH + 10;
        assert_eq!(a.tap(240, y), vec![Action::PlayIndex(ids[2])]);
    }

    // ── Shelf ───────────────────────────────────────────────────────────────────────────────

    #[test]
    fn shelf_go_leaves_a_working_back_button() {
        // Regression: `Go` called go(), which REPLACED the route stack with a single screen, so
        // Back (and the left-edge swipe) did nothing and the user was stranded.
        let mut a = unlocked();
        a.tap(372, 16); // Menu
        a.tap(200, 91 + 63 + 8); // Library
        assert_eq!(a.current(), Screen::Library);
        a.open_shelf();
        a.tap(420, 582); // header Pin → slot 0
        a.tap(240, 200); // close
        a.go(Screen::NowPlaying);
        a.open_shelf();
        a.tap(200, 640 + 12); // slot 0 row body → GO
        assert_eq!(a.current(), Screen::Library);
        assert!(!a.shelf_is_open());
        assert_eq!(a.press(Button::Back), vec![]);
        assert_eq!(a.current(), Screen::NowPlaying, "Back must work after a pin jump");
    }

    #[test]
    fn shelf_empty_slot_pins_and_filled_slot_goes() {
        let mut a = unlocked();
        a.tap(372, 16);
        a.tap(200, 91 + 63 + 8); // Library
        a.open_shelf();
        // Tap the BODY of empty slot 2 → pins there specifically (it used to pin into slot 0 or,
        // in the "GO" column, silently close the sheet).
        a.tap(200, 640 + 2 * 46 + 12);
        assert!(a.pins[2].is_some());
        assert!(a.pins[0].is_none());
        assert!(a.toast.starts_with("Pinned to slot 3"), "{}", a.toast);
        assert!(a.shelf_is_open(), "pinning must not dismiss the sheet");
        // The × column forgets it.
        a.tap(440, 640 + 2 * 46 + 12);
        assert!(a.pins[2].is_none());
    }

    #[test]
    fn shelf_restores_the_whole_place_not_just_the_screen() {
        let mut a = unlocked();
        // A library tall enough to actually scroll (the design sample fits on one screen).
        let mut lib = Library::sample();
        lib.songs = (0..120)
            .map(|i| SongRow { title: format!("Track {i:03}"), object_id: i, ..Default::default() })
            .collect();
        a.set_library(lib);
        a.stack = vec![Screen::Library];
        a.lib_tab = Tab::Songs;
        a.lib_sort = 2;
        a.scroll_px(140);
        let scroll = a.lib_scroll_px;
        assert!(scroll > 0);
        a.open_shelf();
        a.tap(420, 582); // Pin
        a.tap(240, 200); // close
        // Wander off: different tab, different sort, scrolled back to the top.
        a.lib_tab = Tab::Albums;
        a.lib_sort = 0;
        a.lib_scroll_px = 0;
        a.go(Screen::NowPlaying);
        a.open_shelf();
        a.tap(200, 640 + 12); // GO
        assert_eq!(a.current(), Screen::Library);
        assert_eq!(a.lib_tab, Tab::Songs);
        assert_eq!(a.lib_sort, 2);
        assert_eq!(a.lib_scroll_px, scroll, "the list position is part of 'the place'");
    }

    #[test]
    fn shelf_pins_survive_a_reboot() {
        let mut a = unlocked();
        a.stack = vec![Screen::Library];
        a.lib_tab = Tab::Artists;
        a.lib_sort = 1;
        a.lib_scroll_px = 96;
        a.open_shelf();
        a.tap(420, 582);
        let encoded = a.shelf_pin_encode(0);
        assert!(!encoded.is_empty());
        // A fresh boot restores it from the config line.
        let mut b = unlocked();
        b.shelf_pin_decode(0, &encoded);
        assert_eq!(b.pins[0], a.pins[0]);
        // Garbage in a hand-edited config clears the slot rather than panicking.
        b.shelf_pin_decode(0, "not|a|pin");
        assert!(b.pins[0].is_none());
        b.shelf_pin_decode(1, "");
        assert!(b.pins[1].is_none());
    }

    #[test]
    fn shelf_swallows_drags_meant_for_the_sheet() {
        // Regression: a drag on the modal sheet scrolled (and flung) the list behind it, so the
        // Shelf appeared frozen while the screen moved underneath.
        let mut a = unlocked();
        a.stack = vec![Screen::Library];
        a.scroll_px(200);
        let before = a.lib_scroll_px;
        a.open_shelf();
        a.scroll_px(300);
        a.fling(2000.0);
        a.tick();
        assert_eq!(a.lib_scroll_px, before, "the sheet must own the gesture");
    }

    // ── Settings: scroll + UI scale slider ──────────────────────────────────────────────────

    #[test]
    fn settings_about_rows_are_reachable() {
        // Regression: the ABOUT section was laid out past y=800 with no scrolling, so "Firmware"
        // was half off the panel and "Model" was entirely off it — and `row_at` agreed, so they
        // could never be tapped either.
        let mut a = unlocked();
        a.stack = vec![Screen::Settings];
        for _ in 0..crate::settings::ROWS {
            a.press(Button::Down);
        }
        assert_eq!(a.settings_sel, crate::settings::ROW_MODEL);
        assert!(a.settings_scroll_px > 0, "the cursor must scroll the list");
        // The last row is now inside the window and hit-testable.
        let top = crate::settings::row_top_px(crate::settings::ROW_MODEL);
        let y = crate::settings::LIST_TOP + top - a.settings_scroll_px + 10;
        assert!(y < crate::settings::LIST_BOTTOM);
        assert_eq!(crate::settings::row_at(y, a.settings_scroll_px), Some(crate::settings::ROW_MODEL));
    }

    #[test]
    fn settings_scrolls_by_drag_and_clamps() {
        let mut a = unlocked();
        a.stack = vec![Screen::Settings];
        let max = crate::settings::max_scroll_px();
        assert!(max > 0, "the settings list is taller than the panel");
        a.scroll_px(10_000);
        assert_eq!(a.settings_scroll_px, max);
        a.scroll_px(-10_000);
        assert_eq!(a.settings_scroll_px, 0);
    }

    #[test]
    fn ui_scale_slider_scrubs_taps_and_steps() {
        let _scale = lock_scale();
        let mut a = unlocked();
        crate::text::set_scale_pct(100);
        a.stack = vec![Screen::Settings];
        let row_y = crate::settings::LIST_TOP + crate::settings::row_top_px(crate::settings::ROW_UI_SCALE) + 10;
        // A tap on the track jumps straight to that stop (SeekBar idiom, not tap-to-cycle).
        let acts = a.tap(460, row_y);
        assert_eq!(acts, vec![Action::UiScaleChanged]);
        assert_eq!(crate::text::scale_pct(), *crate::text::SCALE_STEPS.last().unwrap());
        // Dragging it scrubs live.
        assert!(a.scrub_begin(100, row_y));
        a.scrub_move(100);
        assert_eq!(crate::text::scale_pct(), crate::text::SCALE_STEPS[0]);
        assert_eq!(a.scrub_end(), vec![Action::UiScaleChanged]);
        // Buttons step one stop and clamp at both ends.
        a.settings_sel = crate::settings::ROW_UI_SCALE;
        a.press(Button::Right);
        assert_eq!(crate::text::scale_pct(), crate::text::SCALE_STEPS[1]);
        for _ in 0..20 {
            a.press(Button::Left);
        }
        assert_eq!(crate::text::scale_pct(), crate::text::SCALE_STEPS[0]);
        for _ in 0..20 {
            a.press(Button::Right);
        }
        assert_eq!(crate::text::scale_pct(), *crate::text::SCALE_STEPS.last().unwrap());
    }

    #[test]
    fn text_scale_keeps_measure_and_draw_in_step() {
        let _scale = lock_scale();
        // The safety property behind the slider: `fit`/`center`/`right` all resolve through
        // measure(), so a scale that measure() ignored would silently break every truncation.
        let fonts = FontSet::load();
        let st = crate::widgets::sty(crate::text::Family::Sans, crate::text::Weight::Bold, 20.0,
                                     Theme::day().ink, 0.0);
        crate::text::set_scale_pct(100);
        let base = crate::text::measure(&fonts, "Atlas Hands", &st);
        crate::text::set_scale_pct(140);
        let big = crate::text::measure(&fonts, "Atlas Hands", &st);
        assert!(big > base * 1.3, "measure() must follow the scale ({base} -> {big})");
        // draw() advances the pen by the same amount it measured.
        let mut c = Canvas::new();
        let pen = crate::text::draw(&mut c, &fonts, 0.0, 40.0, "Atlas Hands", &st);
        assert!((pen - big).abs() < 0.01, "draw() pen {pen} != measure() {big}");
        crate::text::set_scale_pct(100);
    }

    #[test]
    fn library_tab_taps_land_on_the_labels_that_are_drawn() {
        let _scale = lock_scale();
        // Regression: the tab strip was LAID OUT from measured label widths but HIT-TESTED
        // against hardcoded thresholds (x<120/220/330). At the default size "ALBUMS" is drawn at
        // x≈94..154, so tapping its left half selected SONGS. This checks the midpoint of every
        // drawn label selects that label — at three UI scales, since the labels move with the
        // scale and fixed thresholds could not have followed.
        let fonts = FontSet::load();
        for pct in [80u32, 100, 140] {
            crate::text::set_scale_pct(pct);
            let mut a = unlocked();
            a.stack = vec![Screen::Library];
            let mut c = Canvas::new();
            a.render(&mut c, &fonts, &sample_np()); // caches the drawn strip
            for (tab, x, w) in library::tab_layout(&fonts) {
                let mid = (x + w / 2.0) as i32;
                a.tap(mid, library::TAB_TOP + 10);
                assert_eq!(a.lib_tab, tab, "scale {pct}%: tap at x={mid} picked the wrong tab");
            }
        }
        crate::text::set_scale_pct(100);
    }

    // ── Bluetooth: real link state ──────────────────────────────────────────────────────────

    #[test]
    fn bluetooth_screen_reports_the_real_link() {
        // Regression: the screen showed a CONNECTED card naming "WH-1000XM5" whenever the UI
        // toggle was on, whether or not anything was actually connected.
        let mut a = unlocked();
        assert!(a.bt_on);
        assert!(!a.bt_connected(), "no link until the shell reports one");
        assert!(!a.bt_link_known(), "and we say we don't know, rather than guessing");
        assert_eq!(a.bt_peer(), None);
        assert!(a.set_bt_link(1, "WH-1000XM4"));
        assert_eq!(a.bt_peer(), Some("WH-1000XM4"));
        assert!(a.bt_link_known());
        assert!(!a.set_bt_link(1, "WH-1000XM4"), "no change → no repaint");
        // A link with no resolvable name still reads as connected, honestly.
        assert!(a.set_bt_link(1, ""));
        assert_eq!(a.bt_peer(), Some("Connected device"));
        // Dropping the link clears the card.
        assert!(a.set_bt_link(0, ""));
        assert_eq!(a.bt_peer(), None);
        assert!(a.bt_link_known());
        // And "we can't tell on this firmware" is its own state, not a fake disconnect.
        assert!(a.set_bt_link(-1, ""));
        assert!(!a.bt_link_known());
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
