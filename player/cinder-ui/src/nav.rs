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
    /// One artist's page: their albums + every track, pushed from an Artists-tab row.
    Artist,
    Playlist,
    UpNext,
    Eq,
    Sound,
    Bluetooth,
    /// Paired-device picker (connect / disconnect / forget). Pushed from Bluetooth ▸ "Pair new
    /// device"; before 2026-07-30 `pairing.rs` rendered but had no route at all.
    Pairing,
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
/// Where a newly queued track goes. PlayerService has no insert operation, so this only decides
/// where it lands in OUR queue — the sequence it plays from is rebuilt at a track boundary (see
/// `Action::QueueChanged`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum QueueAt {
    /// Straight after the current track. Swipe LEFT on a row.
    Next,
    /// At the end of whatever is already queued. Swipe RIGHT — the original gesture, kept so the
    /// habit still works and still means the same thing.
    Later,
}

/// Side effects the shell carries out (via cinder-audio / system services). The UI emits these
/// instead of acting on audio itself.
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
    /// Shuffle ONE named artist: the shuffle button on an Artists row, or the band on that
    /// artist's page. The payload is a `lib.artists` INDEX (resolve it with
    /// `App::artist_name_at`) rather than the name, because `Action` is `Copy` and a `String`
    /// would take that away from every other variant.
    ShuffleArtist(usize),
    /// Shuffle one playlist by DB id. Same channel as `PlayPlaylist`, but shuffled — the page's
    /// band needs it and `ShuffleScope::Playlist` picks a RANDOM playlist, which is a different
    /// thing entirely.
    ShufflePlaylist(i64),
    ThemeChanged(bool),
    Sleep,
    EnterUsbMsc,
    ExitUsbMsc, // leave USB mass-storage: the shell remounts the volume + restores the USB mode
    EqChanged([i8; 10]), // shell applies the band gains to the sound DSP
    BtToggle(bool),      // shell turns the BT transmitter on/off
    /// Drop the CURRENT link but leave the radio on, so the device stays paired and reconnectable.
    /// Distinct from `BtToggle(false)`, which powers the radio down — the Disconnect button used to
    /// emit that, so tapping it turned Bluetooth off entirely instead of hanging up on one device.
    BtDisconnect,
    /// Connect a specific PAIRED device by its row on the Devices screen. The shell keeps the BD
    /// addresses in the same order it pushed the names, so the index is the whole payload — the UI
    /// never handles a raw address. → `BtTransmitterServiceClient::RequestConnection`.
    BtConnectDevice(usize),
    /// Drop a device's link key → `BtCommonServiceClient::DeleteLinkkey`. Two-tap confirmed in the
    /// UI, because re-pairing needs the stock player (Cinder cannot scan yet).
    BtForgetDevice(usize),
    /// Re-read the paired list off the radio (`GetPairedDeviceInfo`) and push it back. Emitted when
    /// the Devices screen opens and after a connect/forget, so the list is never stale on screen.
    BtPairedRefresh,
    /// Start (true) or stop (false) discovery → `SetSearchMode`. Results arrive asynchronously on the
    /// `BtCommonServiceListener` and are pushed back as the found-list.
    BtScanToggle(bool),
    /// Pair with a DISCOVERED device by its row in the found-list → `BtCommonServiceClient::Pairing`.
    BtPairDevice(usize),
    /// Answer the pairing prompt: yes → `SetNumericComparison(addr, true)` / `RequestSspReply(…, true)`,
    /// no → the same with false (or `CancelPairing` for a display-only passkey). The UI never handles
    /// the address — the shell replies to whatever the notification carried.
    BtPromptConfirm,
    BtPromptCancel,
    BatteryCareChanged(bool), // shell calls PowerMgrServiceClient::EnableItawariCharging
    SoundChanged,             // shell reads cinder_get_sound_flags + applies via EffectCtrlDmp
    SoundBypass(bool),        // A/B: true = bypass whole chain (B), false = re-enable (A)
    SleepTimer(u32),          // arm/cancel the sleep timer: minutes (0 = off); cinder-ffi counts down
    ShuffleToggle,            // Now Playing shuffle on/off (FFI holds the state; PlayController wiring is device-gated)
    RepeatCycle,              // Now Playing repeat: off ↔ one (shell applies via SetOneTrackMode)
    /// The user queue changed. The shell does NOT re-issue the sequence now — it marks the change
    /// pending and applies it at the next track boundary. Measured on device 2026-07-28: a
    /// SetTrackSequence during playback restarts the sequence (position 9000 → 0) and the track
    /// stops, so an immediate apply would interrupt the music every time you queued anything.
    QueueChanged,
    Restart,                  // confirmed in the modal: shell calls PowerMgrServiceClient::Reboot
    PowerOff,                 // confirmed in the modal: shell calls SetStatus(PowerOff)
    BtCodecChanged,           // device-wide BT transmit codec / LDAC quality changed; shell reads + applies
    /// "Use Enhanced Mode" toggled: shell reads `bt_enhanced()` and calls
    /// `BtTransmitterServiceClient::SetControlAbsoluteVolume` (slot 31).
    BtEnhancedChanged,
    UsbDacToggle(bool),       // engage/disengage USB-DAC input routed to 3.5mm + BT/LDAC (the headline feature)
    /// SEEK within the current track, as permille (0..1000) of its duration. Emitted by the Now
    /// Playing progress rail. On device the shell drives the rail through cinder-ffi (which knows
    /// the duration in ms and suppresses position updates mid-drag); this variant is what the
    /// host sim sees, so both surfaces share ONE claim site — `App::scrub_begin`.
    Seek(u16),
    /// Play the USER queue (swipe-to-queue) starting at index `n`. The shell resolves the whole
    /// queue to a track sequence, so the transport then steps through what Up Next is showing.
    PlayQueueAt(usize),
    /// The UI text scale changed (Settings ▸ UI scale). Internal + persisted; no device call.
    UiScaleChanged,
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

/// A pinned place on the Shelf: enough route context to jump straight back — and that means the
/// *whole* place, not just the screen. It used to store only `screen`/`lib_tab`/`album_view`, so
/// "jump back to where I was" dropped you at the top of the list with the wrong sort and no
/// accordion, which is most of why the Shelf felt broken. Pins now persist across boots too
/// (serialised into cinder_settings.conf by the shell).
#[derive(Clone, PartialEq, Debug)]
struct ShelfPin {
    screen: Screen,
    lib_tab: Tab,
    lib_sort: usize,
    album_sort: usize,
    album_expanded: Option<usize>,
    lib_scroll_px: i32,
    album_view: usize,
    album_scroll_px: i32,
    artist_view: usize,
    playlist_view: usize,
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
            | Screen::Artist
            | Screen::Playlist
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

/// A short stable token for a Screen, for persisting Shelf pins across boots. Tokens are part of
/// the on-disk config format — rename one and every saved pin pointing at it silently clears.
fn screen_token(s: Screen) -> &'static str {
    match s {
        Screen::NowPlaying => "np",
        Screen::Library => "lib",
        Screen::Album => "album",
        Screen::Artist => "artist",
        Screen::Playlist => "playlist",
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
        "artist" => Screen::Artist,
        "playlist" => Screen::Playlist,
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

/// Which library tab is at screen-x `x`, using the strip as it was last drawn? Splits the gaps
/// between labels down the middle, so no pixel of the strip is dead. `None` only when nothing has
/// been rendered yet, which a real tap can't precede.
fn tab_zone_at(zones: &[(Tab, f32, f32)], x: i32) -> Option<Tab> {
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

/// Rail x → permille (0..1000). Shares `now_playing::rail_fraction` with the draw, so the preview
/// position and the drawn handle can never disagree.
fn rail_permille(x: i32) -> u16 {
    (crate::now_playing::rail_fraction(x) * 1000.0).round() as u16
}

/// What the user's finger is currently dragging along a horizontal control.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Scrub {
    None,
    /// Now Playing progress rail → seek.
    Progress,
    /// Settings ▸ UI scale slider.
    UiScale,
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
    /// Artist drill-in: which `lib.artists` row is open, plus its track cursor and pixel scroll.
    /// Held as an INDEX, not a resolved page — the page borrows the library, so it is rebuilt per
    /// frame from this (cheap: it walks the album groups once and collects references).
    artist_view: usize,
    artist_track_idx: usize,
    artist_scroll_px: i32,
    /// Playlist drill-in: the `lib.playlists` index being viewed, plus its cursor and pixel scroll.
    playlist_view: usize,
    playlist_track_idx: usize,
    playlist_scroll_px: i32,
    /// The row currently travelling under a horizontal swipe, and how far. `y` is the screen y the
    /// gesture started at; the renderer resolves it to a row by containment. `None` between
    /// gestures. After release this keeps decaying to zero for the snap-back.
    swipe_row: Option<crate::library::SwipeRow>,
    /// True while the finger is still down on that swipe. Release clears it, which is what lets
    /// `tick_dt` take over and animate the row home.
    swipe_live: bool,
    /// Up Next: pixel scroll of the USER queue, and the row being dragged to a new position (the
    /// grab-handle gesture). `None` between drags.
    queue_scroll_px: i32,
    queue_drag: Option<crate::up_next::QueueDrag>,
    /// The sequence that is PLAYING — the album, playlist or shuffle scope resolved when the user
    /// started it — and where inside it the current track sits. Kept SEPARATE from `queue`, which
    /// is the user's own picks: playing an album does not "queue" it, it sets the context, and a
    /// swipe-queued song belongs in front of whatever the context would have played next.
    ///
    /// Until 2026-08-11 these were one list. `set_pending` dropped the whole resolved album into
    /// `queue`, so Up Next drew the album twice (once as NEXT IN QUEUE, once as NEXT FROM ALBUM)
    /// and a queued track had nothing to jump ahead of — it just landed in the same flat list.
    context: Vec<SongRow>,
    context_idx: usize,
    /// Seed for `queue_shuffle`; advanced on every use so repeated shuffles differ.
    shuffle_seed: u64,
    /// Set by the "keep queue" answer: the next sequence handed to `set_play_context` is APPENDED
    /// rather than replacing what is there. Consumed on use.
    queue_keep: bool,
    /// Scrollbar drag: the scroll offset and the finger y the grab started at, so travel is applied
    /// against a fixed anchor rather than accumulated per event.
    sbar: Option<(i32, i32)>,
    /// Fling (momentum) velocity in px/s for the current scrollable list; decays each tick.
    fling_v: f32,
    /// Hardware volume (0..VOL_MAX steps) + frames the volume HUD stays visible. This is the
    /// 3.5 mm level specifically — the CXD3778GF master, which is where the rocker lands whenever
    /// audio is going out the jack.
    volume: u8,
    vol_overlay: u8,
    /// Bluetooth output volume, kept SEPARATE from `volume` on its own 0..BT_VOL_MAX scale, and
    /// which of the two the rocker is currently driving. They are separate because they are
    /// physically different attenuators: `volume` is the local codec, `bt_volume` is a step count
    /// the sink applies at the far end. Sharing one number would mean connecting headphones
    /// silently reassigns the jack's level, and disconnecting them blasts whatever the headphones
    /// were set to out the jack. `bt_route` is owned by the shell (it polls the radio) — the UI
    /// only reads it.
    bt_volume: u8,
    bt_route: bool,
    /// Name of the currently connected Bluetooth device, as the shell last read it from
    /// `BtTransmitterServiceClient::GetConnectInformation`. `None` = nothing connected. This was
    /// pinned to `None` for as long as there was no safe way to make that call — it takes TWO
    /// out-params by reference and passing one crashed the service client twice.
    bt_connected: Option<String>,
    /// Has the shell reported the link state at all yet? Until it has, the Bluetooth screen says
    /// so rather than claiming "No device connected" — which before the first poll is a guess.
    bt_link_known: bool,
    /// Every device the radio holds a link key for, pushed by the shell from
    /// `GetPairedDeviceInfo`. The UI owns no addresses — a row's index IS the handle, and the shell
    /// keeps its own address vector in the same order, so the two cannot disagree unless the list is
    /// re-read between the tap and the call (which is why a refresh follows every action).
    bt_paired: Vec<crate::pairing::PairedDevice>,
    /// Devices the radio has *discovered* but not paired with, pushed by the shell from
    /// `OnNotifySearchedDevice`, plus whether a scan is currently running. Same index-is-the-handle
    /// arrangement as `bt_paired`, against the shell's own found-list.
    bt_found: Vec<crate::pairing::PairedDevice>,
    bt_scanning: bool,
    /// A pairing prompt the radio is waiting on, pushed by the shell from the listener. While this is
    /// `Some` the Devices screen is modal — see `pairing::hit_prompt`.
    bt_prompt: Option<crate::pairing::Prompt>,
    /// Row whose FORGET is armed (two-tap), and the row with a connect in flight. Both are transient
    /// UI state, cleared whenever the list is replaced.
    bt_forget_armed: Option<usize>,
    bt_connecting: Option<usize>,
    /// Spinner phase in SECONDS for the "connecting"/"scanning" indicators. Advanced by tick_dt
    /// from real elapsed time (never a frame count — this project has already been bitten once by
    /// assuming 60 fps when the device renders at ~32), and only while something is actually in
    /// flight, so an idle screen stays completely static and repaints nothing.
    bt_busy_phase: f32,
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
    /// Sony's "Use Enhanced Mode" — `BtTransmitterService::SetControlAbsoluteVolume`. Persisted
    /// intent; the shell pushes it at the radio on change and after every reconnect. Default ON:
    /// Sony's own help text is "select this check box if you cannot change the volume", and the
    /// off path (VOLUME_UP/VOLUME_DOWN key events) makes sinks beep at every step.
    bt_enhanced: bool,
    /// Runtime fact, not intent: does the connected sink accept absolute volume
    /// (`IsSupportedAbsoluteVolume`)? Pushed by the shell, like `bt_link_known`.
    bt_enhanced_supported: bool,
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
    /// The confirmation modal, when one is open. `Some` makes it modal: it is drawn over the
    /// current screen and it swallows every tap until it is answered.
    confirm: Option<crate::confirm::Ask>,
    /// The song a QueueOnPlay prompt is about. Held only while that modal is up: the tap has
    /// already happened, but which action it becomes depends on the answer.
    pending_song: Option<i64>,
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
    /// The last VISIBLE brightness level. Level 0 (backlight off) is a transient state, not a
    /// setting: it is never persisted, and the shell restores this value on the next input. So
    /// there is no way — not even across a reboot — to end up on a black panel with no way back.
    brightness_restore: u8,
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
    /// Which horizontal control owns the current contact, if any. ONE claim site for both
    /// sliders in the UI (the Now Playing rail and the Settings UI-scale row): the shell and the
    /// sim both ask `scrub_begin` first, and a claim bypasses tap/swipe/vertical-drag entirely.
    scrub: Scrub,
    /// Live rail position (permille) while a Progress scrub is in flight.
    scrub_permille: u16,
    /// The library tab strip as last DRAWN: (tab, x, width). Taps resolve through this, so the
    /// zones always match the labels on screen. (`render` takes `&self`, hence the interior
    /// mutability — the same render-mirrors-hit rule the lists already follow.)
    lib_tab_zones: std::cell::RefCell<Vec<(Tab, f32, f32)>>,
    /// Object ids of the Up Next rows the last render actually drew, in drawn order. The window
    /// auto-scrolls to follow playback, so the renderer (which knows `np`) publishes this for the
    /// hit test instead of `tap` trying to recompute it.
    /// The context index the auto-follow last snapped to, so `render` can tell a track change from
    /// a redraw. Everything else Up Next needs now comes straight off `context`/`queue`, so no hit
    /// test depends on a frame having been drawn first.
    up_next_cur: Option<usize>,
    /// Does the queue still follow playback? True until the user scrolls it themselves; reset on
    /// every entry to the screen. This is the whole of "the queue follows the current song".
    queue_follow: bool,
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
            artist_view: 0,
            playlist_view: 0,
            playlist_track_idx: 0,
            playlist_scroll_px: 0,
            artist_track_idx: 0,
            artist_scroll_px: 0,
            swipe_row: None,
            swipe_live: false,
            queue_scroll_px: 0,
            queue_drag: None,
            context: Vec::new(),
            context_idx: 0,
            shuffle_seed: 0x9E3779B97F4A7C15,
            queue_keep: false,
            sbar: None,
            fling_v: 0.0,
            volume: 15,
            vol_overlay: 0,
            bt_volume: 15,
            bt_route: false,
            bt_connected: None,
            bt_link_known: false,
            bt_paired: Vec::new(),
            bt_found: Vec::new(),
            bt_scanning: false,
            bt_prompt: None,
            bt_forget_armed: None,
            bt_connecting: None,
            bt_busy_phase: 0.0,
            eq_bands: data::EQ_PRESETS[3].1, // "A1"
            eq_sel: 0,
            eq_preset: 3,
            bt_on: true,
            bt_codec: 0,        // LDAC
            bt_ldac_quality: 0, // Auto
            bt_enhanced: true,
            bt_enhanced_supported: true,
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
            confirm: None,
            pending_song: None,
            liked_count: 0,
            settings_scroll_px: 0,
            screen_off_idx: 0,
            screen_off_s: 0,  // OFF by default — an idle blank is opt-in
            brightness: 4,   // matches the shell's ~70% day default
            brightness_restore: 4,
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
            lib_tab_zones: std::cell::RefCell::new(Vec::new()),
            up_next_cur: None,
            queue_follow: true,
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

    /// The Power button was held down (~1 s), Sony's own gesture: open the Power menu.
    ///
    /// Returns whether it opened, so the shell knows whether the eventual RELEASE should still
    /// toggle the screen. Refused while Hold is engaged — the whole point of the switch is that a
    /// pocket cannot operate the device, and a power-off menu is the last thing that should be
    /// reachable in there. Refused too if a modal is already up, so a second hold cannot stack one
    /// dialog on another or silently replace the question you were reading.
    pub fn power_held(&mut self) -> bool {
        if self.locked || self.confirm.is_some() {
            return false;
        }
        self.confirm = Some(crate::confirm::Ask::PowerMenu);
        true
    }

    /// Every "play this library track" goes through here.
    ///
    /// With an empty user queue it is just `PlayIndex`. With tracks queued it is a question first:
    /// playing something new either discards the queue you built by hand or leaves it to play
    /// afterwards, and silently picking either one is wrong. Apple asks the same thing.
    ///
    /// A funnel rather than a check at each call site: `PlayIndex` is emitted from seven places
    /// (song rows, album rows, search, playlists, shuffle, the shelf), and a prompt that only some
    /// of them respect is worse than no prompt — it would look like the queue is discarded at
    /// random depending on which screen you started from.
    fn start_play(&mut self, id: i64) -> Vec<Action> {
        // Only HAND-BUILT picks are worth interrupting for — and since the queue/context split,
        // that is exactly what `queue` holds. It used to also contain the whole playing sequence,
        // which is why a separate `queue_user_adds` counter had to exist to tell the two apart;
        // with the lists separated, the queue being non-empty IS the question.
        if self.queue.is_empty() {
            return vec![Action::PlayIndex(id)];
        }
        self.pending_song = Some(id);
        self.confirm = Some(crate::confirm::Ask::QueueOnPlay);
        vec![]
    }

    /// Test helper: put one track in the user queue.
    #[cfg(test)]
    fn queue_push_for_test(&mut self) {
        self.queue.push(crate::model::SongRow::default());
        // A hand-queued track — which is what the replace prompt is about.
    }

    /// Is a modal dialog currently up? The shell uses this to decide whether the screen-blank
    /// timer should keep running (it must not blank a "Power off?" prompt out from under a finger).
    pub fn modal_open(&self) -> bool {
        self.confirm.is_some()
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
            // ALBUMS TAB: name the OPEN ALBUM, not the tab. The Albums tab is an accordion — the
            // only way to "be at" an album without leaving the list is to have it expanded — so a
            // pin taken there used to read "Library / Albums" and was indistinguishable from any
            // other Albums pin. Naming the album is what makes pinning one from the list useful,
            // and `restore_pin` turns it into a jump-to. (Tapping the artwork still opens the full
            // Album screen, which was always pinnable; this covers the drop-down.)
            Screen::Library => match (self.lib_tab, self.album_expanded) {
                (Tab::Albums, Some(flat)) => match self.lib.albums_flat().get(flat) {
                    Some(al) => (al.name.clone(), al.artist.clone()),
                    None => ("Library".into(), tab_name(self.lib_tab).into()),
                },
                _ => ("Library".into(), tab_name(self.lib_tab).into()),
            },
            Screen::Album => match self.lib.albums_flat().get(self.album_view) {
                Some(al) => (al.name.clone(), al.artist.clone()),
                None => ("Album".into(), String::new()),
            },
            Screen::Artist => match self.lib.artists.get(self.artist_view) {
                Some(ar) => (ar.name.clone(), format!("{} albums · {} tracks", ar.albums, ar.tracks)),
                None => ("Artist".into(), String::new()),
            },
            Screen::Playlist => match self.playlist_row() {
                Some(p) => (p.name.clone(), format!("{} tracks", p.track_list.len())),
                None => ("Playlist".into(), String::new()),
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
            artist_view: self.artist_view,
            playlist_view: self.playlist_view,
            title,
            sub,
        }
    }

    /// Restore a pinned place. Two things this deliberately does NOT do any more: it no longer
    /// calls `go()` (which replaced the whole route stack, stranding the user with a dead Back
    /// button), and it no longer restores only the screen — the list position, sort and expansion
    /// come back too. Back from a restored pin returns to Now Playing.
    fn restore_pin(&mut self, p: &ShelfPin) {
        self.lib_tab = p.lib_tab;
        self.lib_sort = p.lib_sort.min(library::SORTS.len().saturating_sub(1));
        self.album_sort = p.album_sort.min(library::ALBUM_SORTS.len().saturating_sub(1));
        self.album_expanded = p.album_expanded;
        self.album_view = p.album_view;
        self.artist_view = p.artist_view;
        self.playlist_view = p.playlist_view;
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
        self.artist_scroll_px = 0;
        self.playlist_scroll_px = 0;
        // JUMP TO THE PINNED ALBUM. A saved pixel offset is only right for the library that was
        // on disk when the pin was taken — add or remove an album above it and the same offset
        // lands somewhere else entirely. When the pin names an expanded album, recompute the
        // scroll from that album's CURRENT position instead, so the restore puts it at the top of
        // the view every time.
        if p.screen == Screen::Library && p.lib_tab == Tab::Albums {
            if let Some(flat) = p.album_expanded {
                if let Some(rank) = self.album_rank_of(flat) {
                    self.lib_idx = rank;
                    let top = library::row_top_px(
                        Tab::Albums, &self.lib, rank, self.album_sort, self.album_expanded);
                    self.lib_scroll_px = top.clamp(0, self.lib_max_scroll());
                }
            }
        }
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

    /// Pop a transient toast (shelf feedback, queue confirmations).
    fn notify(&mut self, msg: &str) {
        self.toast = msg.to_string();
        self.toast_frames = TOAST_FRAMES;
    }

    // ── Shelf pin persistence ───────────────────────────────────────────────────────────────
    // Pins were session-scoped, so every reboot wiped the user's bookmarks — the one thing a
    // "pin this place" feature must not do. They serialise to one line per slot; the shell
    // stores them in cinder_settings.conf alongside the rest.

    /// Serialise slot `i` as a `|`-separated record. Empty string = the slot is empty. `|` and
    /// newlines are stripped from the labels so a track title can't corrupt the config.
    pub fn shelf_pin_encode(&self, i: usize) -> String {
        let Some(p) = self.pins.get(i).and_then(|p| p.as_ref()) else { return String::new() };
        let clean = |s: &str| s.replace(['|', '\n'], " ");
        format!(
            "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
            screen_token(p.screen),
            tab_token(p.lib_tab),
            p.lib_sort,
            p.album_sort,
            p.album_expanded.map(|e| e as i64).unwrap_or(-1),
            p.lib_scroll_px,
            p.album_view,
            p.album_scroll_px,
            p.artist_view,
            p.playlist_view,
            clean(&p.title),
            clean(&p.sub),
        )
    }

    /// Restore slot `i` from `shelf_pin_encode` output. A malformed or short record CLEARS the
    /// slot rather than failing — a hand-edited config must never keep the player from booting.
    pub fn shelf_pin_decode(&mut self, i: usize, s: &str) {
        if i >= self.pins.len() {
            return;
        }
        let f: Vec<&str> = s.split('|').collect();
        if f.len() < 12 {
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
            artist_view: num(f[8]).max(0) as usize,
            playlist_view: num(f[9]).max(0) as usize,
            title: f[10].to_string(),
            sub: f[11].to_string(),
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
            self.boot_stock_armed = false;   // never leave a restart armed across a screen change
            // Arriving at Up Next always shows you the current track. Scrolling hands the list to
            // the user for as long as they stay on it; leaving and coming back is the reset, which
            // means there is no state to explain and no "jump to current" button to find.
            if s == Screen::UpNext {
                self.queue_follow = true;
                self.up_next_cur = None;   // forces the next render to treat this as a track change
            }
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
            // Select steps the UI scale one stop (a tap on the row uses the x position instead —
            // see `tap`, which treats this row as a slider track).
            crate::settings::ROW_UI_SCALE => self.step_ui_scale(1),
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
            crate::settings::ROW_RESTART => {
                self.confirm = Some(crate::confirm::Ask::Restart);
                vec![]
            }
            crate::settings::ROW_POWER_OFF => {
                self.confirm = Some(crate::confirm::Ask::PowerOff);
                vec![]
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
                // 1..5, then 0 = BACKLIGHT OFF, then wrap to 1. The shell turns the level into a
                // raw backlight value. Level 0 is deliberately the LAST stop before the wrap, so
                // you pass through every visible setting before reaching the invisible one.
                self.brightness = match self.brightness {
                    0 => 1,
                    5 => 0,
                    n => n + 1,
                };
                if self.brightness != 0 {
                    self.brightness_restore = self.brightness;
                }
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
        // MODAL FIRST, and it consumes the tap whatever the answer. Letting a tap fall through to
        // the screen underneath is how a dialog dismissal also presses whatever it was covering.
        if let Some(ask) = self.confirm {
            self.confirm = None;
            return match crate::confirm::hit(ask, x, y) {
                crate::confirm::Hit::Cancel => { self.pending_song = None; vec![] }
                // The menu's own rows say which action was chosen; a yes/no card's Confirm means
                // "the thing the card is named after".
                crate::confirm::Hit::Restart => vec![Action::Restart],
                crate::confirm::Hit::PowerOff => vec![Action::PowerOff],
                crate::confirm::Hit::ClearQueue => {
                    let had = !self.queue.is_empty();
                    self.queue.clear();
                    let mut acts = Vec::new();
                    if had { acts.push(Action::QueueChanged); }
                    if let Some(id) = self.pending_song.take() { acts.push(Action::PlayIndex(id)); }
                    acts
                }
                crate::confirm::Hit::KeepQueue => {
                    // Honour it for real: the new sequence is APPENDED after the picks instead of
                    // replacing them, so "keep" keeps them where the user can still see them.
                    self.queue_keep = true;
                    self.pending_song.take().map(|id| vec![Action::PlayIndex(id)]).unwrap_or_default()
                }
                crate::confirm::Hit::Confirm => match ask {
                    crate::confirm::Ask::Restart => vec![Action::Restart],
                    crate::confirm::Ask::PowerOff => vec![Action::PowerOff],
                    // The menus never produce a bare Confirm (their hit test returns named rows).
                    _ => vec![],
                },
            };
        }
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
                        let id = album.track_list.get(row).map(|s| s.object_id);
                        return id.map(|i| self.start_play(i)).unwrap_or_default();
                    }
                    // The "Play album" band: play from track 1 — the shell's album_context
                    // expands it to the whole album in order. (Hit-tested through the band's own
                    // rect; the old literal range started 16px above where the band is drawn.)
                    if library::hit_album_play_band(x, y) {
                        self.album_track_idx = 0;
                        let id = album.track_list.first().map(|s| s.object_id);
                        return id.map(|i| self.start_play(i)).unwrap_or_default();
                    }
                }
                vec![]
            }
            Screen::Artist => self.tap_artist(x, y),
            Screen::Playlist => self.tap_playlist(x, y),
            Screen::UpNext => {
                use crate::up_next::Slot;
                // CLEAR belongs to the user queue and is only drawn when there is one.
                if !self.queue.is_empty() && crate::up_next::hit_clear_chip(x, y) {
                    self.notify("Queue cleared");
                    return self.queue_clear();
                }
                if crate::up_next::hit_shuffle_chip(x, y) {
                    return self.queue_shuffle();
                }
                // The grab-handle column belongs to the reorder drag. Swallowed rather than
                // treated as a tap: playing a track because a reorder came out too short to
                // classify is a nasty surprise.
                if crate::up_next::queue_grip_hit(x)
                    && matches!(self.up_next_layout().at(y, self.queue_scroll_px), Some(Slot::Queued(_)))
                {
                    return vec![];
                }
                match self.up_next_layout().at(y, self.queue_scroll_px) {
                    // Play the QUEUE from here, not the tapped track's album — `start_play` hands
                    // PlayerService the track's album context, which would make Up Next show one
                    // list while the transport stepped through another.
                    Some(Slot::Queued(i)) => vec![Action::PlayQueueAt(i)],
                    // The playing row is not a destination; it is where you already are.
                    Some(Slot::Current(_)) => {
                        self.go(Screen::NowPlaying);
                        vec![]
                    }
                    // History and upcoming are both album tracks, so both just play that track.
                    // Tapping upward is how you go back a song without stepping through every one.
                    Some(Slot::History(i)) | Some(Slot::Upcoming(i)) => {
                        match self.context.get(i).map(|t| t.object_id) {
                            Some(id) => {
                                // Playing something new re-arms the follow, so the list snaps to
                                // the new track instead of staying where the finger left it.
                                self.queue_follow = true;
                                self.start_play(id)
                            }
                            None => vec![],
                        }
                    }
                    // A section heading, or the empty state: keep the old shortcut home.
                    _ => {
                        self.go(Screen::NowPlaying);
                        vec![]
                    }
                }
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
                    // The UI-scale row is a SLIDER TRACK: the tap's x picks the stop directly
                    // (the SeekBar idiom) rather than cycling one step per tap.
                    if row == crate::settings::ROW_UI_SCALE {
                        crate::text::set_scale_idx(crate::settings::ui_scale_idx_at(x));
                        return vec![Action::UiScaleChanged];
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
                            *g = (*g + crate::eq::BAND_STEP).min(crate::eq::BAND_MAX);
                        } else {
                            *g = (*g - crate::eq::BAND_STEP).max(-crate::eq::BAND_MAX);
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
                    BtHit::Toggle => {
                        self.bt_on = !self.bt_on;
                        vec![Action::BtToggle(self.bt_on)]
                    }
                    // Hang up on the device, leave the radio ON. This shared the Toggle arm until
                    // 2026-07-29, which meant the Disconnect button powered Bluetooth down — the
                    // device then also stopped being reconnectable without re-enabling the radio.
                    BtHit::Disconnect => vec![Action::BtDisconnect],
                    BtHit::Codec(i) => {
                        self.bt_codec = i as u8;
                        vec![Action::BtCodecChanged]
                    }
                    BtHit::Quality(i) => {
                        self.bt_ldac_quality = i as u8;
                        vec![Action::BtCodecChanged]
                    }
                    BtHit::Enhanced => {
                        self.bt_enhanced = !self.bt_enhanced;
                        vec![Action::BtEnhancedChanged]
                    }
                    // Open the paired-device picker and re-read the list on the way in, so what is
                    // on screen is what the radio actually holds link keys for right now.
                    BtHit::Pair => {
                        self.push(Screen::Pairing);
                        vec![Action::BtPairedRefresh]
                    }
                    BtHit::None => vec![],
                }
            }
            Screen::Pairing => {
                use crate::pairing::PairHit;
                // A prompt takes the whole screen: the radio is blocked waiting for an answer, so
                // nothing else on this screen may be tapped until it gets one.
                if let Some(p) = self.bt_prompt.clone() {
                    return match crate::pairing::hit_prompt(x, y, p.kind) {
                        PairHit::PromptConfirm => {
                            self.bt_prompt = None;
                            vec![Action::BtPromptConfirm]
                        }
                        PairHit::PromptCancel => {
                            self.bt_prompt = None;
                            vec![Action::BtPromptCancel]
                        }
                        _ => vec![],
                    };
                }
                match crate::pairing::hit(x, y, self.bt_paired.len(), self.bt_found.len()) {
                    PairHit::Scan => {
                        self.bt_forget_armed = None;
                        self.bt_scanning = !self.bt_scanning;
                        if self.bt_scanning {
                            self.bt_found.clear();
                        }
                        vec![Action::BtScanToggle(self.bt_scanning)]
                    }
                    PairHit::Pair(i) => {
                        self.bt_forget_armed = None;
                        if i < self.bt_found.len() {
                            vec![Action::BtPairDevice(i)]
                        } else {
                            vec![]
                        }
                    }
                    PairHit::Row(i) => {
                        self.bt_forget_armed = None; // any other tap disarms a pending FORGET
                        match self.bt_paired.get(i) {
                            // Already connected → hang up. Same call the Bluetooth screen's
                            // Disconnect makes, so there is one code path for "drop the link".
                            Some(d) if d.connected => vec![Action::BtDisconnect],
                            Some(_) => {
                                self.bt_connecting = Some(i);
                                vec![Action::BtConnectDevice(i)]
                            }
                            None => vec![],
                        }
                    }
                    PairHit::Forget(i) => {
                        if i >= self.bt_paired.len() {
                            return vec![];
                        }
                        if self.bt_forget_armed == Some(i) {
                            self.bt_forget_armed = None;
                            // The refresh is emitted by the shell side after the delete lands; we
                            // do not remove the row locally, because a failed DeleteLinkkey would
                            // then show a device as gone while the radio still has it.
                            vec![Action::BtForgetDevice(i)]
                        } else {
                            self.bt_forget_armed = Some(i);
                            vec![]
                        }
                    }
                    // `hit()` never returns these — the prompt has its own hit test above, and it
                    // runs first because a prompt is modal.
                    PairHit::PromptConfirm | PairHit::PromptCancel | PairHit::None => vec![],
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
        if (library::TAB_TOP..library::TAB_BOT).contains(&y) {
            // Resolve against the strip as it was LAST DRAWN. These used to be hardcoded x
            // thresholds that did not match the measured layout — at the default size "ALBUMS" is
            // drawn at x≈94..154, so tapping its left half selected SONGS, and the same drift ran
            // down the rest of the strip. Fixed thresholds could not have followed the UI scale
            // either. See `library::tab_layout`.
            let Some(tab) = self.tab_at_cached(x) else { return vec![] };
            self.lib_tab = tab;
            self.lib_idx = 0;
            self.lib_scroll_px = 0;
            self.fling_v = 0.0;
            self.album_expanded = None;
            return vec![];
        }
        // A–Z rail: right edge, over the list. Tested BEFORE the rows, because it overlays them —
        // a tap there means "jump", never "open the row underneath". Skipped entirely when the
        // active sort has no alphabetical ordering: the rail isn't drawn then, so it must not go on
        // silently eating taps over rows the user can see.
        if library::az_hit_x(x)
            && library::az_key_for(self.lib_tab, self.lib_sort, self.album_sort).is_some()
        {
            if let Some(letter) = library::az_letter_at(y, self.lib_tab) {
                if let Some(px) = library::az_scroll_for(
                    self.lib_tab, &self.lib, letter, self.lib_sort, self.album_sort,
                    self.album_expanded,
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
                    self.start_play(id)
                })
                .unwrap_or_default(),
            // A playlist row opens that playlist's page, the same way an artist row opens theirs.
            // The button on the right of the row plays the whole list from the top in saved order
            // without the detour — the shortcut the row itself used to be.
            Tab::Playlists => {
                self.lib_idx = row;
                let Some(p) = self.lib.playlists.get(row) else { return vec![] };
                if x >= 404 {
                    return vec![Action::PlayPlaylist(p.id)];
                }
                self.open_playlist(row);
                vec![]
            }
            // An artist row has no track under the finger — it opens that artist's page.
            // The shuffle button on the right of the row shuffles them without the detour.
            Tab::Artists => {
                self.lib_idx = row;
                if row >= self.lib.artists.len() {
                    return vec![];
                }
                if x >= 404 {
                    return vec![Action::ShuffleArtist(row)];
                }
                self.open_artist(row);
                vec![]
            }
            _ => {
                self.lib_idx = row;
                vec![]
            }
        }
    }

    /// Push the artist page for `lib.artists[idx]`, from its top.
    fn open_artist(&mut self, idx: usize) {
        self.artist_view = idx;
        self.artist_track_idx = 0;
        self.artist_scroll_px = 0;
        self.fling_v = 0.0;
        self.push(Screen::Artist);
    }

    /// The open artist's page, resolved against the library. Borrows `self.lib`, so callers that
    /// need to mutate must copy what they want out first.
    fn artist_page(&self) -> Option<crate::library::ArtistPage<'_>> {
        self.lib.artists.get(self.artist_view).map(|a| library::artist_page(&self.lib, &a.name))
    }

    /// A tap on the artist page: an album row drills in, a track row plays.
    fn tap_artist(&mut self, x: i32, y: i32) -> Vec<Action> {
        if library::hit_artist_shuffle_band(x, y) {
            return vec![Action::ShuffleArtist(self.artist_view)];
        }
        // Resolve the hit and copy the result out — `page` borrows `self.lib`, and everything
        // below this line mutates `self`.
        let hit = self.artist_page().and_then(|p| {
            library::artist_hit(&p, self.artist_scroll_px, y).map(|h| match h {
                library::ArtistHit::Track(i) => (h, p.tracks.get(i).map(|t| t.song.object_id)),
                _ => (h, None),
            })
        });
        match hit {
            Some((library::ArtistHit::Album(flat), _)) => {
                self.album_view = flat;
                self.album_track_idx = 0;
                self.album_scroll_px = 0;
                self.fling_v = 0.0;
                self.push(Screen::Album);
                vec![]
            }
            Some((library::ArtistHit::Track(i), Some(id))) => {
                self.artist_track_idx = i;
                self.start_play(id)
            }
            _ => vec![],
        }
    }

    /// Push the playlist page for `lib.playlists[idx]`, from its top.
    fn open_playlist(&mut self, idx: usize) {
        self.playlist_view = idx;
        self.playlist_track_idx = 0;
        self.playlist_scroll_px = 0;
        self.fling_v = 0.0;
        self.push(Screen::Playlist);
    }

    /// The open playlist, or None if the library changed under us.
    fn playlist_row(&self) -> Option<&crate::model::PlaylistRow> {
        self.lib.playlists.get(self.playlist_view)
    }

    /// A tap on the playlist page: the band shuffles it, a track row plays it.
    fn tap_playlist(&mut self, x: i32, y: i32) -> Vec<Action> {
        let Some(id) = self.playlist_row().map(|p| p.id) else { return vec![] };
        if library::hit_playlist_shuffle_band(x, y) {
            return vec![Action::ShufflePlaylist(id)];
        }
        // Copy the object id out before mutating — `playlist_row` borrows `self.lib`.
        let hit = self.playlist_row().and_then(|p| {
            library::playlist_hit_track(p, self.playlist_scroll_px, y)
                .and_then(|i| p.track_list.get(i).map(|s| (i, s.object_id)))
        });
        match hit {
            Some((i, oid)) => {
                self.playlist_track_idx = i;
                self.start_play(oid)
            }
            None => vec![],
        }
    }

    /// The track under `y` on the playlist page, for the swipe-to-queue gesture.
    fn playlist_track_at(&self, y: i32) -> Option<SongRow> {
        let p = self.playlist_row()?;
        library::playlist_hit_track(p, self.playlist_scroll_px, y)
            .and_then(|i| p.track_list.get(i).cloned())
    }

    /// The track under `y` on the artist page, for the swipe-to-queue gesture.
    fn artist_track_at(&self, y: i32) -> Option<SongRow> {
        let page = self.artist_page()?;
        match library::artist_hit(&page, self.artist_scroll_px, y) {
            Some(library::ArtistHit::Track(i)) => page.tracks.get(i).map(|t| t.song.clone()),
            _ => None,
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
            Some(AlbumsHit::Track(flat, track)) => {
                let id = self.lib.albums_flat().get(flat)
                    .and_then(|al| al.track_list.get(track)).map(|s| s.object_id);
                id.map(|i| self.start_play(i)).unwrap_or_default()
            }
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

    /// Which library tab is at screen-x `x`, from the strip as last rendered. Falls back to an
    /// even quarter split if nothing has been drawn yet (unreachable in practice — a tap cannot
    /// precede the first paint).
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
        tab_zone_at(&zones, x)
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
        // started on the sheet scrolled (and flung) the list behind it, which read as the Shelf
        // "not working" — the sheet sat still while the screen moved underneath it.
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
            Screen::Artist => {
                if let Some(max) = self.artist_page().map(|p| library::artist_max_scroll_px(&p)) {
                    self.artist_scroll_px = (self.artist_scroll_px + dy_px).clamp(0, max);
                }
            }
            Screen::Playlist => {
                if let Some(max) = self.playlist_row().map(library::playlist_max_scroll_px) {
                    self.playlist_scroll_px = (self.playlist_scroll_px + dy_px).clamp(0, max);
                }
            }
            Screen::Settings => {
                let max = crate::settings::max_scroll_px();
                self.settings_scroll_px = (self.settings_scroll_px + dy_px).clamp(0, max);
            }
            // The user queue drew from row 0 and stopped at the bottom of the panel, so anything
            // past ~10 tracks was unreachable — and unreorderable with it.
            Screen::UpNext => {
                let max = self.up_next_layout().max_scroll_px();
                let next = (self.queue_scroll_px + dy_px).clamp(0, max);
                // A deliberate scroll takes the list over from the auto-follow. Without this the
                // next track change would yank it straight back and the user could never read
                // ahead. Re-entering the screen re-arms it (see `push`).
                if next != self.queue_scroll_px {
                    self.queue_follow = false;
                }
                self.queue_scroll_px = next;
            }
            _ => {}
        }
    }

    /// Momentum fling: the release velocity (px/s, same sign convention as `scroll_px`). The
    /// per-frame `tick()` integrates and decays it, keeping frames dirty until it stops.
    pub fn fling(&mut self, velocity_px_s: f32) {
        if self.shelf_open || self.locked {
            return; // the modal sheet owns the gesture — see scroll_px
        }
        if matches!(self.current(),
                    Screen::Library | Screen::Album | Screen::Artist | Screen::Playlist | Screen::UpNext) {
            self.fling_v = velocity_px_s.clamp(-8000.0, 8000.0);
        }
    }

    // ── Horizontal scrub: the progress rail (seek) and the UI-scale slider ──────────────────
    // Every touch-down is offered here FIRST — by `cinder_scrub_hit` on device and by the sim's
    // pointer classifier on the host. A claim routes the whole gesture to `scrub_move`/`scrub_end`
    // and the tap/swipe/vertical-drag classification never sees it. Keeping the claim in ONE
    // place is the point: the shell owns the seek's millisecond math (it knows the duration and
    // must suppress incoming position updates mid-drag), but it must not own a second, drifting
    // copy of "is the finger on the rail".

    /// A finger went down at (x, y). True if it grabbed a scrubbable control.
    pub fn scrub_begin(&mut self, x: i32, y: i32) -> bool {
        self.scrub = Scrub::None;
        if self.locked || self.shelf_open {
            return false;
        }
        match self.current() {
            Screen::NowPlaying => {
                let band = (crate::now_playing::RAIL_GRAB_TOP..=crate::now_playing::RAIL_GRAB_BOT)
                    .contains(&y);
                if band && (0..=crate::canvas::W as i32).contains(&x) {
                    self.scrub = Scrub::Progress;
                    self.scrub_permille = rail_permille(x);
                    true
                } else {
                    false
                }
            }
            Screen::Settings => {
                if crate::settings::row_at(y, self.settings_scroll_px)
                    == Some(crate::settings::ROW_UI_SCALE)
                {
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

    /// The finger moved to `x` during a scrub. The rail only PREVIEWS here (committing a seek per
    /// motion event would hammer PlayerService); the UI-scale slider applies live.
    pub fn scrub_move(&mut self, x: i32) -> Vec<Action> {
        match self.scrub {
            Scrub::Progress => {
                self.scrub_permille = rail_permille(x);
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

    /// Is a scrub in progress?
    pub fn is_scrubbing(&self) -> bool {
        self.scrub != Scrub::None
    }

    /// Is the in-flight scrub the UI-scale slider? The shell branches on this: a Progress scrub
    /// is resolved to milliseconds in cinder-ffi, a UiScale one is applied and persisted here.
    pub fn scrub_is_ui_scale(&self) -> bool {
        self.scrub == Scrub::UiScale
    }

    /// Step the UI text scale by `d` stops (Left/Right on the Settings slider row).
    fn step_ui_scale(&mut self, d: i32) -> Vec<Action> {
        let n = crate::text::SCALE_STEPS.len() as i32;
        let idx = (crate::text::scale_idx() as i32 + d).clamp(0, n - 1) as usize;
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

    /// Jump straight to a screen for the host preview harness (no route history). Test/preview
    /// only — the app itself always arrives via `go`/`push` so Back stays meaningful.
    pub fn go_for_preview(&mut self, s: Screen) {
        self.go(s);
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
    /// LIVE horizontal drag on a list row: `dx` is total travel from the gesture's start point,
    /// `y` that start point. The row under `y` moves with the finger and reveals what releasing
    /// will do. Returns true if a row actually took the gesture, so the shell can commit the
    /// contact to it (and stop trying to promote it into a vertical scroll).
    ///
    /// This is feedback only — it changes no state but the offset. The queueing still happens on
    /// release, in `swipe`, so a gesture dragged back to zero costs nothing.
    pub fn swipe_track(&mut self, dx: i32, y: i32) -> bool {
        if self.locked || self.shelf_open || self.confirm.is_some() {
            return false;
        }
        // Only where a track sits under the finger: the same rows `swipe` can queue. Artist and
        // playlist rows have nothing to queue, so they must not move — a row that slides and then
        // does nothing is worse feedback than a row that never moved.
        let has_track = match self.current() {
            Screen::Library => match self.lib_tab {
                Tab::Songs => library::hit_row(self.lib_tab, &self.lib, self.lib_scroll_px, y)
                    .and_then(|r| library::song_at(&self.lib, self.lib_sort, r))
                    .is_some(),
                Tab::Albums => self.albums_track_at(y).is_some(),
                _ => false,
            },
            Screen::Album => self
                .lib
                .albums_flat()
                .get(self.album_view)
                .and_then(|al| library::album_hit_track(al, self.album_scroll_px, y))
                .is_some(),
            Screen::Artist => self.artist_track_at(y).is_some(),
            Screen::Playlist => self.playlist_track_at(y).is_some(),
            // The queue's own rows swipe too, but to REMOVE rather than to queue again.
            // Only the USER-QUEUE rows swipe, and they swipe to REMOVE. The album rows around
            // them are the album's own order — there is nothing to remove them from.
            Screen::UpNext => matches!(
                self.up_next_layout().at(y, self.queue_scroll_px),
                Some(crate::up_next::Slot::Queued(_))
            ),
            _ => false,
        };
        if !has_track {
            return false;
        }
        self.swipe_live = true;
        self.swipe_row = Some(crate::library::SwipeRow { y, dx: library::swipe_offset(dx) });
        true
    }

    /// The finger came off a live row swipe. The row animates home from wherever it was; whether
    /// the gesture also queued anything is `swipe`'s business, not this one's.
    pub fn swipe_release(&mut self) {
        self.swipe_live = false;
    }

    /// The row offset currently in effect, for tests and the host preview.
    pub fn swipe_state(&self) -> Option<crate::library::SwipeRow> {
        self.swipe_row
    }

    /// Does a vertical drag starting at `(x, y)` pick up a queue row for reordering?
    ///
    /// Ownership is decided by the START point, the same rule the scrub rail uses: the handle owns
    /// the contact for its whole life, and a drag that begins anywhere else scrolls the list even
    /// if it later wanders across the handle column. Returns true when the row was picked up, so
    /// the shell knows to stream this contact here instead of to the scroll.
    pub fn reorder_begin(&mut self, x: i32, y: i32) -> bool {
        if self.locked || self.shelf_open || self.confirm.is_some() {
            return false;
        }
        // Only the USER-QUEUE rows reorder. The album rows sharing this list are the album's own
        // order, which is not ours to rewrite — and the layout is what tells the two apart now.
        if self.current() != Screen::UpNext || self.queue.is_empty() {
            return false;
        }
        if !crate::up_next::queue_grip_hit(x) {
            return false;
        }
        let Some(crate::up_next::Slot::Queued(from)) =
            self.up_next_layout().at(y, self.queue_scroll_px)
        else {
            return false;
        };
        // The row's screen top comes from the LAYOUT, not from `from * RH` — the queue section no
        // longer starts at the top of the list, so that arithmetic would grab the wrong offset and
        // the lifted row would jump under the finger.
        let Some(content_top) = self.up_next_layout().top_of(crate::up_next::Slot::Queued(from))
        else {
            return false;
        };
        let row_top = crate::chrome::HEADER_BOTTOM + content_top - self.queue_scroll_px;
        self.fling_v = 0.0; // a pick-up must not ride a leftover flick
        self.queue_drag = Some(crate::up_next::QueueDrag {
            from,
            to: from,
            start_y: y,
            y,
            grab_off: y - row_top,
        });
        true
    }

    /// Stream the drag. `dy` is total travel from the gesture's start point.
    pub fn reorder_track(&mut self, dy: i32) {
        let Some(mut d) = self.queue_drag else { return };
        d.y = d.start_y + dy;
        d.to = self.up_next_layout().queue_slot_for(d.float_top(), self.queue_scroll_px);
        self.queue_drag = Some(d);
    }

    /// Drop the row. Returns `QueueChanged` when the order actually moved.
    pub fn reorder_release(&mut self) -> Vec<Action> {
        let Some(d) = self.queue_drag.take() else { return vec![] };
        self.queue_move(d.from, d.to)
    }

    /// The drag in effect, for tests and the host preview.
    pub fn reorder_state(&self) -> Option<crate::up_next::QueueDrag> {
        self.queue_drag
    }

    /// The Up Next slot list. Rebuilt on demand from the three numbers `render` publishes, rather
    /// than cached from the last frame: a hit test that depended on a frame having been drawn is a
    /// hit test that silently resolves against the wrong list the first time it runs.
    fn up_next_layout(&self) -> crate::up_next::Layout {
        crate::up_next::layout(
            self.context.len(),
            (!self.context.is_empty()).then_some(self.context_idx),
            self.queue.len(),
        )
    }

    /// `(max_scroll_px, list_top)` of whatever the current screen scrolls, or None if it doesn't.
    /// The scrollbar's whole geometry follows from these two, so this is the only place a screen
    /// has to be taught about the drag.
    fn sbar_metrics(&self) -> Option<(i32, i32)> {
        match self.current() {
            Screen::Library => Some((self.lib_max_scroll(), library::list_top(self.lib_tab))),
            Screen::Album => self
                .lib
                .albums_flat()
                .get(self.album_view)
                .map(|al| (library::album_max_scroll_px(al), library::ALBUM_TRACKS_TOP)),
            Screen::Artist => self
                .artist_page()
                .map(|p| (library::artist_max_scroll_px(&p), library::artist_content_top())),
            Screen::Playlist => self
                .playlist_row()
                .map(|p| (library::playlist_max_scroll_px(p), library::playlist_content_top())),
            // The bar rides the WHOLE list now (history + current + queue + album), not just the
            // user queue — which is also why it appears on a plain album view, where the old
            // row-stepping window offered no way to drag at all.
            Screen::UpNext => Some((
                self.up_next_layout().max_scroll_px(),
                crate::chrome::HEADER_BOTTOM,
            )),
            _ => None,
        }
    }

    /// Does a vertical drag starting at `(x, y)` grab the scrollbar? Sony's own UI lets you drag
    /// the bar, and a flick-and-wait is a poor substitute on a 3400-song list.
    ///
    /// The strip is shared with the A–Z rail and split by GESTURE, not geometry: this is only
    /// consulted for a drag, and `tap` still routes the same x to a letter jump.
    pub fn sbar_begin(&mut self, x: i32, y: i32) -> bool {
        if self.locked || self.shelf_open || self.confirm.is_some() {
            return false;
        }
        if !library::sbar_hit_x(x) {
            return false;
        }
        let Some((max, top)) = self.sbar_metrics() else { return false };
        // Nothing to scroll, or the thumb fills the track: no drag, so the contact stays available
        // to the list underneath.
        if max <= 0 || !(top..library::list_bottom()).contains(&y) {
            return false;
        }
        if library::sbar_span(top, max + (library::list_bottom() - top)) <= 0 {
            return false;
        }
        self.fling_v = 0.0;
        self.sbar = Some((self.sbar_scroll(), y));
        true
    }

    /// Current scroll of the screen the scrollbar is riding.
    fn sbar_scroll(&self) -> i32 {
        match self.current() {
            Screen::Album => self.album_scroll_px,
            Screen::Artist => self.artist_scroll_px,
            Screen::Playlist => self.playlist_scroll_px,
            Screen::UpNext => self.queue_scroll_px,
            _ => self.lib_scroll_px,
        }
    }

    /// Stream the scrollbar drag. `dy` is TOTAL travel from the grab point.
    ///
    /// Finger px are converted to content px through the thumb's travel, so the content tracks the
    /// THUMB rather than the finger — dragging the bar half way down lands half way through the
    /// list however long it is. Applied against the anchor captured at grab time, so a coalesced
    /// event stream can't accumulate drift.
    pub fn sbar_track(&mut self, dy: i32) {
        let Some((start_scroll, _)) = self.sbar else { return };
        let Some((max, top)) = self.sbar_metrics() else { return };
        let span = library::sbar_span(top, max + (library::list_bottom() - top));
        if span <= 0 {
            return;
        }
        let want = start_scroll + (dy as i64 * max as i64 / span as i64) as i32;
        let delta = want.clamp(0, max) - self.sbar_scroll();
        self.scroll_px(delta);
    }

    pub fn sbar_release(&mut self) {
        self.sbar = None;
    }

    /// Is a scrollbar drag in progress? The renderer widens and accents the thumb while it is, so
    /// the (much wider) grab zone gives some feedback that it took the gesture.
    pub fn sbar_active(&self) -> bool {
        self.sbar.is_some()
    }

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
                // A RANGE, not just an upper bound. The shell passes y = 0 when a contact somehow
                // arrives with no ABS_Y, and 0 is above the artwork (it is the status bar) — a bare
                // `y < BOT` would silently turn every one of those degenerate swipes into a page
                // turn instead of the track skip it has always been.
                if (crate::now_playing::PAGE_TOP..crate::now_playing::PAGE_SWIPE_BOT).contains(&y) {
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
            // LEFT swipe on a row = Play Next. The mirror of the right-swipe below, which has
            // always meant "queue this", and which keeps meaning exactly that. Two symmetric
            // gestures need no new control and no long-press, and the toast names which one
            // happened so a mis-swipe is legible rather than silent.
            Screen::Library if dir < 0 => {
                let song = match self.lib_tab {
                    Tab::Songs => library::hit_row(self.lib_tab, &self.lib, self.lib_scroll_px, y)
                        .and_then(|rank| library::song_at(&self.lib, self.lib_sort, rank))
                        .cloned(),
                    Tab::Albums => self.albums_track_at(y),
                    _ => None,
                };
                match song {
                    Some(s) => self.enqueue_at(s, y, QueueAt::Next),
                    None => vec![],
                }
            }
            Screen::Album if dir < 0 => {
                let song = self.lib.albums_flat().get(self.album_view).and_then(|al| {
                    library::album_hit_track(al, self.album_scroll_px, y)
                        .and_then(|ti| al.track_list.get(ti).cloned())
                });
                match song {
                    Some(s) => self.enqueue_at(s, y, QueueAt::Next),
                    None => vec![],
                }
            }
            // On the queue itself, either direction removes the row — there is nothing to queue.
            Screen::UpNext => {
                match self.up_next_layout().at(y, self.queue_scroll_px) {
                    Some(crate::up_next::Slot::Queued(i)) if i < self.queue.len() => {
                        let gone = self.queue.remove(i);
                        self.notify(&format!("Removed — {}", gone.title));
                        // The list just got shorter, so the scroll may now be past its end.
                        self.queue_scroll_px =
                            self.queue_scroll_px.min(self.up_next_layout().max_scroll_px());
                        vec![Action::QueueChanged]
                    }
                    _ => vec![],
                }
            }
            // The playlist page's rows queue exactly like every other track list.
            Screen::Playlist => match self.playlist_track_at(y) {
                Some(s) => {
                    let at = if dir < 0 { QueueAt::Next } else { QueueAt::Later };
                    self.enqueue_at(s, y, at)
                }
                None => vec![],
            },
            // The artist page's track rows queue exactly like every other track list.
            Screen::Artist => match self.artist_track_at(y) {
                Some(s) => {
                    let at = if dir < 0 { QueueAt::Next } else { QueueAt::Later };
                    self.enqueue_at(s, y, at)
                }
                None => vec![],
            },
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
                match song {
                    Some(s) => self.enqueue_at(s, y, QueueAt::Later),
                    None => vec![],
                }
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
        self.enqueue_at(s, y, QueueAt::Later);
    }

    /// Add a track to the user queue, at the front or the back.
    ///
    /// The toast names WHICH it was, because the two gestures are mirror images and the only way to
    /// know a left-swipe registered as "next" rather than "later" is to be told.
    fn enqueue_at(&mut self, s: SongRow, y: i32, at: QueueAt) -> Vec<Action> {
        self.toast = match at {
            QueueAt::Next => format!("Playing next — {}", s.title),
            QueueAt::Later => format!("Added to queue — {}", s.title),
        };
        self.toast_frames = TOAST_FRAMES;
        self.queue_anim_y = y;
        self.queue_anim_frames = QUEUE_ANIM_FRAMES;
        match at {
            QueueAt::Next => self.queue.insert(0, s),
            QueueAt::Later => self.queue.push(s),
        }
        vec![Action::QueueChanged]
    }

    /// Move a queued track. Used by reordering; clamps rather than panicking on a stale index,
    /// because the queue can change under a gesture that started before it did.
    pub fn queue_move(&mut self, from: usize, to: usize) -> Vec<Action> {
        if from >= self.queue.len() || to >= self.queue.len() || from == to {
            return vec![];
        }
        let row = self.queue.remove(from);
        self.queue.insert(to, row);
        vec![Action::QueueChanged]
    }

    /// Remove one row from the user queue — used by the shell when a queued track STARTS, so a
    /// pick is consumed rather than replayed on the next re-issue.
    pub fn queue_remove(&mut self, i: usize) {
        if i < self.queue.len() {
            self.queue.remove(i);
            self.queue_scroll_px = self.queue_scroll_px.min(self.up_next_layout().max_scroll_px());
        }
    }

    /// Drop everything queued. The "clear it?" answer when you play something unrelated.
    pub fn queue_clear(&mut self) -> Vec<Action> {
        if self.queue.is_empty() {
            return vec![];
        }
        self.queue.clear();
        self.queue_scroll_px = 0;
        self.queue_drag = None;
        vec![Action::QueueChanged]
    }

    /// Set the PLAYBACK CONTEXT — the sequence that is now playing, and which track of it started.
    ///
    /// Called by the shell every time it resolves a play action: a track tap, an album, a playlist,
    /// any Shuffle band. This is not the queue. The queue is what the user asked for by hand, and
    /// it plays FIRST; the context is what follows once those run out.
    pub fn set_play_context(&mut self, rows: Vec<SongRow>, start: usize) {
        self.context_idx = start.min(rows.len().saturating_sub(1));
        self.context = rows;
        // "Keep queue" keeps the USER's picks. It used to append the new sequence after them,
        // which only made sense while the two were one list; now the context is its own thing and
        // keeping the picks is simply not clearing them.
        if !self.queue_keep {
            self.queue.clear();
        }
        self.queue_keep = false;
        self.queue_scroll_px = 0;
        self.queue_drag = None;
        // RE-ARM THE FOLLOW. Every play that does not start from the Up Next screen itself arrives
        // here — a library tap, an album, a playlist, a Shuffle band. Without this, a user who had
        // scrolled Up Next once (which hands the list to them, deliberately) would find it stuck at
        // the top for the rest of the session while a completely different album played, which is
        // exactly the behaviour this screen was reworked to fix.
        self.queue_follow = true;
        self.up_next_cur = None;
    }

    /// The playing sequence and our position in it. The shell reads these to build what it hands
    /// PlayerService: `[current] + queue + context[idx+1..]`.
    pub fn context(&self) -> &[SongRow] {
        &self.context
    }
    pub fn context_idx(&self) -> usize {
        self.context_idx
    }

    /// The shell reports which context track just started, by object_id. Returns true if that
    /// moved us — the caller repaints on true. An id that is not in the context (the user queue
    /// took over, or playback came from somewhere else entirely) leaves the index alone rather
    /// than guessing.
    pub fn set_context_playing(&mut self, object_id: i64) -> bool {
        match self.context.iter().position(|t| t.object_id == object_id) {
            Some(i) if i != self.context_idx => {
                self.context_idx = i;
                true
            }
            _ => false,
        }
    }

    /// Shuffle what is still to come, leaving the current track and the user's own picks alone.
    ///
    /// Only the REMAINDER moves: shuffling the tracks already played would rewrite history for no
    /// reason, and shuffling the user queue would undo an order they chose by hand. Returns the
    /// action that re-issues the sequence, or nothing when there is too little left to shuffle.
    pub fn queue_shuffle(&mut self) -> Vec<Action> {
        let first = self.context_idx + 1;
        if self.context.len().saturating_sub(first) < 2 {
            self.notify("Nothing left to shuffle");
            return vec![];
        }
        // A small xorshift seeded from the shuffle count: self-contained (cinder-ui has no rng and
        // should not gain a dependency for one button) and reproducible in tests.
        self.shuffle_seed = self.shuffle_seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let mut x = self.shuffle_seed | 1;
        let tail = &mut self.context[first..];
        for i in (1..tail.len()).rev() {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            tail.swap(i, (x % (i as u64 + 1)) as usize);
        }
        self.notify("Shuffled what's next");
        vec![Action::QueueChanged]
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
        matches!(s, Screen::Library | Screen::Album | Screen::Artist | Screen::Playlist)
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

    /// Keep the artist page's track cursor in view. The track rows sit below a variable-height
    /// album block, so the row's content-space y comes from the page layout rather than from
    /// `idx * row_h` — the album rows above it are not the same height.
    /// Keep the playlist cursor on screen when the transport buttons move it.
    fn playlist_ensure_visible(&mut self) {
        let want = self.playlist_track_idx as i32 * library::PLAYLIST_TRACK_RH;
        let Some(max) = self.playlist_row().map(library::playlist_max_scroll_px) else { return };
        let view = library::playlist_view_h();
        let s = self.playlist_scroll_px;
        let s = if want < s {
            want
        } else if want + library::PLAYLIST_TRACK_RH > s + view {
            want + library::PLAYLIST_TRACK_RH - view
        } else {
            s
        };
        self.playlist_scroll_px = s.clamp(0, max);
    }

    fn artist_ensure_visible(&mut self) {
        let want = self.artist_track_idx;
        let Some((row_top, max)) = self.artist_page().and_then(|p| {
            let top = p.rows.iter().find_map(|(vy, r)| {
                matches!(*r, library::ArtistRowKind::Song(i) if i == want).then_some(*vy)
            })?;
            Some((top, library::artist_max_scroll_px(&p)))
        }) else {
            return;
        };
        let view = library::artist_view_h().max(1);
        let rh = library::ARTIST_TRACK_RH;
        if row_top < self.artist_scroll_px {
            self.artist_scroll_px = row_top;
        } else if row_top + rh > self.artist_scroll_px + view {
            self.artist_scroll_px = row_top + rh - view;
        }
        self.artist_scroll_px = self.artist_scroll_px.clamp(0, max);
    }

    /// The name of `lib.artists[idx]` — how the shell resolves an `Action::ShuffleArtist` payload
    /// back to something it can query the DB with.
    pub fn artist_name_at(&self, idx: usize) -> Option<&str> {
        self.lib.artists.get(idx).map(|a| a.name.as_str())
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
                    self.vol_step(true);
                    vec![Action::VolUp]
                }
                Button::VolDown => {
                    self.vol_step(false);
                    vec![Action::VolDown]
                }
                _ => vec![],
            };
        }

        // MODAL OPEN: Back dismisses it, and every other button is swallowed. Tapping the dimmed
        // backdrop already cancels, so this is a second escape rather than the only one — but a
        // dialog you cannot back out of is exactly the shape of bug that strands a device with no
        // d-pad, and a transport press leaking through to the music underneath a "Power off?"
        // prompt would be its own small surprise.
        if self.confirm.is_some() {
            if b == Button::Back {
                self.confirm = None;
            }
            return vec![];
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
                    self.vol_step(true);
                    self.vol_overlay = crate::overlay::VOL_FRAMES;
                    vec![Action::VolUp]
                }
                Button::VolDown => {
                    self.vol_step(false);
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
                self.vol_step(true);
                self.vol_overlay = crate::overlay::VOL_FRAMES;
                return vec![Action::VolUp];
            }
            Button::VolDown => {
                self.vol_step(false);
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
                    Tab::Songs => {
                        let id = library::song_at(&self.lib, self.lib_sort, self.lib_idx)
                            .map(|s| s.object_id);
                        id.map(|i| self.start_play(i)).unwrap_or_default()
                    }
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
                    Button::Select => {
                        let id = self.lib.albums_flat().get(self.album_view)
                            .and_then(|a| a.track_list.get(self.album_track_idx))
                            .map(|s| s.object_id);
                        id.map(|i| self.start_play(i)).unwrap_or_default()
                    }
                    Button::Back | Button::Left => {
                        self.pop();
                        vec![]
                    }
                    _ => vec![],
                }
            }
            Screen::Playlist => {
                let n = self.playlist_row().map(|p| p.track_list.len()).unwrap_or(0);
                match b {
                    Button::Up => {
                        self.playlist_track_idx = self.playlist_track_idx.saturating_sub(1);
                        self.playlist_ensure_visible();
                        vec![]
                    }
                    Button::Down => {
                        if self.playlist_track_idx + 1 < n {
                            self.playlist_track_idx += 1;
                            self.playlist_ensure_visible();
                        }
                        vec![]
                    }
                    Button::Select => {
                        let id = self
                            .playlist_row()
                            .and_then(|p| p.track_list.get(self.playlist_track_idx))
                            .map(|s| s.object_id);
                        id.map(|i| self.start_play(i)).unwrap_or_default()
                    }
                    Button::Back | Button::Left => {
                        self.pop();
                        vec![]
                    }
                    _ => vec![],
                }
            }
            Screen::Artist => {
                let n = self.artist_page().map(|p| p.tracks.len()).unwrap_or(0);
                match b {
                    Button::Up => {
                        self.artist_track_idx = self.artist_track_idx.saturating_sub(1);
                        self.artist_ensure_visible();
                        vec![]
                    }
                    Button::Down => {
                        if self.artist_track_idx + 1 < n {
                            self.artist_track_idx += 1;
                            self.artist_ensure_visible();
                        }
                        vec![]
                    }
                    Button::Select => {
                        let id = self
                            .artist_page()
                            .and_then(|p| p.tracks.get(self.artist_track_idx).map(|t| t.song.object_id));
                        id.map(|i| self.start_play(i)).unwrap_or_default()
                    }
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
                    *g = (*g + crate::eq::BAND_STEP).min(crate::eq::BAND_MAX);
                    vec![Action::EqChanged(self.eq_bands)]
                }
                Button::Down => {
                    let g = &mut self.eq_bands[self.eq_sel];
                    *g = (*g - crate::eq::BAND_STEP).max(-crate::eq::BAND_MAX);
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
                // Record the tab strip exactly as drawn, so `tap` hits the labels the user sees.
                *self.lib_tab_zones.borrow_mut() = crate::library::tab_layout(fonts);
                crate::library::render(
                    c, &theme, fonts, self.lib_tab, self.lib_idx, self.lib_scroll_px, self.lib_sort,
                    self.album_sort, self.album_expanded, &self.lib, self.swipe_row,
                    self.sbar_active(),
                );
                crate::library::az_render(
                    c, &theme, fonts, self.lib_tab, &self.lib, self.lib_sort, self.album_sort,
                );
            }
            Screen::Album => {
                let flat = self.lib.albums_flat();
                if let Some(al) = flat.get(self.album_view) {
                    crate::library::album_view(
                        c, &theme, fonts, al, self.album_track_idx, self.album_scroll_px,
                        self.album_cover.as_ref(), self.swipe_row, self.sbar_active(),
                    );
                } else {
                    crate::library::render(
                        c, &theme, fonts, self.lib_tab, self.lib_idx, self.lib_scroll_px, self.lib_sort,
                        self.album_sort, self.album_expanded, &self.lib, self.swipe_row,
                        self.sbar_active(),
                    );
                }
            }
            Screen::Playlist => match self.playlist_row() {
                Some(pl) => crate::library::playlist_view(
                    c, &theme, fonts, &self.lib, pl, self.playlist_scroll_px,
                    self.playlist_track_idx, self.swipe_row, self.sbar_active(),
                ),
                // The library reloaded and the index is stale — fall back to the tab rather than
                // drawing a blank page the user cannot leave.
                None => crate::library::render(
                    c, &theme, fonts, self.lib_tab, self.lib_idx, self.lib_scroll_px, self.lib_sort,
                    self.album_sort, self.album_expanded, &self.lib, self.swipe_row,
                    self.sbar_active(),
                ),
            },
            Screen::Artist => match self.artist_page() {
                Some(page) => crate::library::artist_view(
                    c, &theme, fonts, &self.lib, &page, self.artist_scroll_px,
                    self.artist_track_idx, self.swipe_row, self.sbar_active(),
                ),
                // The artist index outlived its library (a rescan while the page was open).
                // Falling back to the list is better than a blank screen, and Back still works.
                None => crate::library::render(
                    c, &theme, fonts, self.lib_tab, self.lib_idx, self.lib_scroll_px, self.lib_sort,
                    self.album_sort, self.album_expanded, &self.lib, self.swipe_row,
                    self.sbar_active(),
                ),
            },
            Screen::Onboarding => crate::onboarding::render(c, &theme, fonts, self.onboarding_page),
            Screen::UsbStorage => crate::usb_storage::render(c, &theme, fonts),
            Screen::UpNext => {
                // ONE list, Apple Music order: history above, the playing track, then the user's
                // own queue, then the rest of the album. This screen used to be two mutually
                // exclusive views — queueing a single track replaced the album window entirely and
                // took the now-playing row with it, so the queue could not follow playback at all.
                // The CONTEXT is the truth about what is playing — the sequence the shell
                // resolved when this started. Up Next used to re-derive it by searching the
                // library for the now-playing title, which could only ever find an album (never a
                // playlist or a shuffle scope) and got it wrong outright when two albums shared a
                // track name.
                //
                // Cloned rather than borrowed because the auto-follow below has to WRITE
                // `queue_scroll_px`. One sequence's rows per frame, on this screen only.
                let (album, tracks, cur) = if self.context.is_empty() {
                    (String::new(), Vec::new(), None)
                } else {
                    let name = self.context.get(self.context_idx)
                        .map(|t| t.art.clone()).unwrap_or_default();
                    (name, self.context.clone(), Some(self.context_idx))
                };
                // AUTO-FOLLOW. `render` is the only place that knows what is playing, so the snap
                // lives here: whenever the current track moves (or we have just arrived on the
                // screen) and the user has not taken the list over by scrolling it, park NOW
                // PLAYING a third of the way down.
                let track_changed = self.up_next_cur != cur;
                self.up_next_cur = cur;
                let _ = np;   // the context, not the now-playing strings, drives this screen now
                let l = self.up_next_layout();
                if self.queue_follow && track_changed {
                    self.queue_scroll_px = l.follow_scroll();
                    self.fling_v = 0.0;
                }
                let view = crate::up_next::QueueView {
                    album: &album,
                    tracks: &tracks,
                    current: cur,
                    queue: &self.queue,
                    lib: &self.lib,
                    scroll_px: self.queue_scroll_px,
                    drag: self.queue_drag,
                    swipe: self.swipe_row,
                    sbar_active: self.sbar_active(),
                };
                crate::up_next::render_view(c, &theme, fonts, &view);
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
                    // The REAL connected device, pushed by the shell from
                    // GetConnectInformation(vector<uint8_t>& addr, string& name). Before that call
                    // was safe to make this was pinned to None, and before THAT it reported a
                    // hardcoded "WH-1000XM5" whenever the UI-only radio toggle was on — i.e. it
                    // invented a paired device that was never there. bluetooth::render still draws
                    // an honest "No device connected" when it is None.
                    connected: self.bt_connected.as_deref(),
                    // …and until the shell has reported ONCE we don't even claim that much. On
                    // this firmware there is no sysfs/hcitool link node (both absent — measured),
                    // so pst is the only source; before its first poll "no device" would be a
                    // guess dressed as a fact.
                    link_known: self.bt_link_known,
                    codec_sel: self.bt_codec,
                    ldac_quality: self.bt_ldac_quality,
                    enhanced: self.bt_enhanced,
                    enhanced_supported: self.bt_enhanced_supported,
                    connecting: self.bt_connecting.is_some(),
                    busy_phase: self.bt_busy_phase,
                };
                crate::bluetooth::render(c, &theme, fonts, &bt)
            }
            Screen::Pairing => {
                crate::pairing::render(
                    c,
                    &theme,
                    fonts,
                    &self.bt_paired,
                    &self.bt_found,
                    self.bt_forget_armed,
                    self.bt_connecting,
                    self.bt_scanning,
                    self.bt_busy_phase,
                );
                // Drawn last so it sits over the list, matching how the tap handler treats it.
                if let Some(p) = &self.bt_prompt {
                    crate::pairing::render_prompt(c, &theme, fonts, p);
                }
            }
            Screen::Settings => {
                let sleep_lbl = self.sleep_label();
                let brightness_lbl = if self.brightness == 0 {
                    "BACKLIGHT OFF".to_string()
                } else {
                    format!("{} / 5", self.brightness)
                };
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
        // Swipe-to-queue chip riding the flicked row (list screens only — if the user navigates
        // away mid-animation the anchor row is gone, so it just stops). It is anchored to a ROW,
        // so it belongs with the screen: UNDER the Shelf sheet, unlike the transients below.
        if self.queue_anim_frames > 0
            && matches!(self.current(),
                        Screen::Library | Screen::Album | Screen::Artist | Screen::Playlist)
        {
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
            crate::overlay::volume(c, &theme, fonts, self.display_volume());
        }
        if self.toast_frames > 0 && self.current() != Screen::Lock {
            crate::overlay::toast(c, &theme, fonts, &self.toast);
        }

        // The confirmation modal is drawn LAST OF ALL — over every screen, the status strip, the
        // return bar and the shelf. It is modal, so nothing may sit on top of it; anything that did
        // would read as still interactive while every tap is going to the dialog.
        if let Some(ask) = self.confirm {
            crate::confirm::render(c, &theme, fonts, ask);
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
        // Bluetooth busy spinner. Only runs while a connect attempt or a scan is genuinely in
        // flight, so an idle Devices screen costs nothing and repaints nothing. Wrapped at 8s (one
        // whole number of 8-dot revolutions) to keep the f32 exact forever.
        if self.bt_connecting.is_some() || self.bt_scanning {
            self.bt_busy_phase = (self.bt_busy_phase + dt / 1000.0) % 8.0;
            animating = true;
        } else if self.bt_busy_phase != 0.0 {
            self.bt_busy_phase = 0.0;   // one last repaint clears the spinner
            animating = true;
        }
        // Fling momentum: integrate over real time, decay exponentially per unit time (0.92 per
        // 60 fps frame, expressed continuously), stop below a threshold. Hitting the clamp
        // (top/bottom) kills it immediately.
        if self.fling_v != 0.0 {
            let scroll_of = |a: &Self| match a.current() {
                Screen::Library => a.lib_scroll_px,
                Screen::Album => a.album_scroll_px,
                Screen::Artist => a.artist_scroll_px,
                Screen::Playlist => a.playlist_scroll_px,
                Screen::Settings => a.settings_scroll_px,
                Screen::UpNext => a.queue_scroll_px,
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
        // Swipe snap-back: once the finger is off, the row travels home under the same
        // time-based decay the fling uses (NOT a per-call constant — see the comment above; the
        // device renders at ~32 fps, so a per-frame factor would take twice as long there as it
        // does on the host, on the one animation the user is watching most closely).
        if let Some(s) = self.swipe_row {
            if self.swipe_live {
                animating = true; // the finger is driving it; keep frames coming
            } else {
                let dx = (s.dx as f32 * 0.70f32.powf(dt / FRAME_MS as f32)) as i32;
                self.swipe_row = if dx.abs() < 2 { None } else { Some(crate::library::SwipeRow { dx, ..s }) };
                animating = true;
            }
        }
        // Queue reorder: hold the row near an edge and the list scrolls under it, so a track can
        // be moved further than one screenful. Time-based like everything else here, and driven
        // from the tick rather than from touch events — a finger held perfectly still delivers no
        // events at all, which is exactly when this has to keep working.
        if let Some(mut d) = self.queue_drag {
            const EDGE_PX: i32 = 70;
            const EDGE_RATE: f32 = 520.0; // px/s at the very edge, tapering to 0 at EDGE_PX in
            let top = crate::chrome::HEADER_BOTTOM;
            let bot = top + crate::up_next::queue_view_h();
            let into = if d.y < top + EDGE_PX {
                -(top + EDGE_PX - d.y) as f32 / EDGE_PX as f32
            } else if d.y > bot - EDGE_PX {
                (d.y - (bot - EDGE_PX)) as f32 / EDGE_PX as f32
            } else {
                0.0
            };
            if into != 0.0 {
                // Both of these read the UNIFIED layout: the edge-scroll clamps against the whole
                // list (history + current + queue + album), and the landing slot is measured from
                // the queue section's own top, which is no longer the top of the screen.
                let l = self.up_next_layout();
                let step = (into.clamp(-1.0, 1.0) * EDGE_RATE * dt / 1000.0) as i32;
                self.queue_scroll_px =
                    (self.queue_scroll_px + step).clamp(0, l.max_scroll_px());
                // The finger hasn't moved, but the content under it has — so where the row would
                // land has changed and the parted list must follow.
                d.to = l.queue_slot_for(d.float_top(), self.queue_scroll_px);
                self.queue_drag = Some(d);
            }
            animating = true; // the lifted row is live; keep painting it
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
    ///
    /// This stays the 3.5 mm level even while the Bluetooth route is active, which is what lets
    /// the shell leave the codec exactly where the user left it and lets the level persist
    /// unpolluted by a listening session on headphones.
    pub fn volume_level(&self) -> u8 {
        self.volume
    }

    /// Move whichever volume the rocker currently owns. One press is one step on that route's own
    /// scale — on Bluetooth that is one AVRCP command to the sink, so the step count IS the level.
    fn vol_step(&mut self, up: bool) {
        if self.bt_route {
            self.bt_volume = if up {
                (self.bt_volume + 1).min(crate::overlay::BT_VOL_MAX)
            } else {
                self.bt_volume.saturating_sub(1)
            };
        } else {
            self.volume = if up {
                (self.volume + 1).min(crate::overlay::VOL_MAX)
            } else {
                self.volume.saturating_sub(1)
            };
        }
    }

    /// The level the HUD draws, always on the 0..VOL_MAX scale so the bar looks the same on both
    /// routes. Bluetooth's 30 steps are stretched over it; the number under the bar is therefore
    /// the 0..120 equivalent, not the raw AVRCP step.
    pub fn display_volume(&self) -> u8 {
        if self.bt_route {
            ((self.bt_volume as u16 * crate::overlay::VOL_MAX as u16)
                / crate::overlay::BT_VOL_MAX as u16) as u8
        } else {
            self.volume
        }
    }

    /// The Bluetooth route's own level, in AVRCP steps (0..BT_VOL_MAX). Persisted separately from
    /// `volume_level()`; the shell uses it only to know which way it has drifted from the sink.
    pub fn bt_volume_level(&self) -> u8 {
        self.bt_volume
    }

    pub fn set_bt_volume(&mut self, level: u8) {
        self.bt_volume = level.min(crate::overlay::BT_VOL_MAX);
    }

    pub fn bt_route(&self) -> bool {
        self.bt_route
    }

    /// Push the connected device's name (empty = nothing connected). The shell polls
    /// GetConnectInformation and calls this; the Bluetooth screen shows it on the CONNECTED card.
    /// Returns true if the peer changed (so the caller only repaints when it must).
    pub fn set_bt_connected(&mut self, name: Option<&str>) -> bool {
        let next = match name {
            Some(n) if !n.is_empty() => Some(n.to_string()),
            _ => None,
        };
        // The shell has now looked, whatever it found — so "No device connected" stops being a
        // guess and becomes an observation.
        let changed = next != self.bt_connected || !self.bt_link_known;
        self.bt_connected = next;
        self.bt_link_known = true;
        changed
    }

    pub fn bt_connected(&self) -> Option<&str> {
        self.bt_connected.as_deref()
    }

    /// Has the shell reported the link state at least once?
    pub fn bt_link_known(&self) -> bool {
        self.bt_link_known
    }

    /// Replace the paired list wholesale. The shell calls `bt_paired_clear()` then one
    /// `bt_paired_add()` per device, in the order it will index them — so this also drops the
    /// transient row state, which belonged to the OLD ordering and would otherwise point at the
    /// wrong device after a refresh.
    pub fn bt_paired_clear(&mut self) {
        self.bt_paired.clear();
        self.bt_forget_armed = None;
        self.bt_connecting = None;
    }

    pub fn bt_paired_add(&mut self, name: &str, kind: &str, connected: bool) {
        self.bt_paired.push(crate::pairing::PairedDevice {
            name: name.to_string(),
            kind: kind.to_string(),
            connected,
        });
    }

    pub fn bt_paired_len(&self) -> usize {
        self.bt_paired.len()
    }

    /// Replace the discovered-device list. Cleared when a scan starts, then appended to as the
    /// listener reports devices; the shell keeps the addresses in the same order.
    pub fn bt_found_clear(&mut self) {
        self.bt_found.clear();
    }

    pub fn bt_found_add(&mut self, name: &str, kind: &str) {
        self.bt_found.push(crate::pairing::PairedDevice {
            name: name.to_string(),
            kind: kind.to_string(),
            connected: false,
        });
    }

    pub fn bt_found_len(&self) -> usize {
        self.bt_found.len()
    }

    /// Scan state. The shell owns the truth (the radio stops on its own when the search window
    /// expires), so it pushes the answer here rather than the UI assuming its own tap stuck.
    pub fn bt_scanning(&self) -> bool {
        self.bt_scanning
    }

    pub fn set_bt_scanning(&mut self, on: bool) {
        self.bt_scanning = on;
    }

    /// Raise (or clear) the pairing prompt. `kind` 0 clears; otherwise 1 = numeric comparison,
    /// 2 = passkey (display only), 3 = SSP request.
    pub fn set_bt_prompt(&mut self, kind: u8, name: &str, code: u32) {
        self.bt_prompt = if kind == 0 {
            None
        } else {
            Some(crate::pairing::Prompt { kind, name: name.to_string(), code })
        };
    }

    pub fn bt_prompt_kind(&self) -> u8 {
        self.bt_prompt.as_ref().map_or(0, |p| p.kind)
    }

    /// The shell polls the radio and pushes the answer here; the rocker follows it from the next
    /// press onward. Setting it does NOT touch either level — that is the whole point.
    pub fn set_bt_route(&mut self, on: bool) {
        self.bt_route = on;
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

    /// The level to persist and to come back to. Never 0 — see `brightness_restore`.
    pub fn brightness_restore(&self) -> u8 {
        self.brightness_restore
    }

    /// Leave the transient backlight-off state. True if anything changed, so the shell only
    /// rewrites the node when it must.
    pub fn brightness_wake(&mut self) -> bool {
        if self.brightness != 0 {
            return false;
        }
        self.brightness = self.brightness_restore.clamp(1, 5);
        true
    }
    /// Restore a persisted level. Clamps to 1..5 — 0 is a transient state and is never written to
    /// the settings file, so a value of 0 here means a hand-edited or corrupt config, and the
    /// answer to that is a visible panel, not a black one.
    pub fn set_brightness(&mut self, level: u8) {
        self.brightness = level.clamp(1, 5);
        self.brightness_restore = self.brightness;
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
    /// "Use Enhanced Mode" — absolute volume. The shell reads this after `BtEnhancedChanged` and
    /// after every reconnect, and hands it to `SetControlAbsoluteVolume`.
    pub fn bt_enhanced(&self) -> bool {
        self.bt_enhanced
    }
    pub fn set_bt_enhanced(&mut self, on: bool) {
        self.bt_enhanced = on;
    }
    /// Report what the sink can actually do. Returns whether the screen needs a repaint, so the
    /// shell's poll doesn't dirty a frame every tick.
    pub fn set_bt_enhanced_supported(&mut self, on: bool) -> bool {
        let changed = self.bt_enhanced_supported != on;
        self.bt_enhanced_supported = on;
        changed
    }
    /// USB-DAC mode engaged? The shell reads this after a UsbDacToggle action to start/stop the
    /// LDAC bridge + switch the USB gadget to UAC (without tearing down Bluetooth).
    pub fn usb_dac_on(&self) -> bool {
        self.usb_dac_on
    }

    pub fn bt_on(&self) -> bool {
        self.bt_on
    }

    /// Force the Bluetooth switch to match the radio's real state, without raising a `BtToggle`.
    ///
    /// Same reasoning as [`Self::set_usb_dac`]: the switch is intent, `GetBtStatus` is fact. The
    /// shell reconciles at startup so the switch cannot claim the radio is on when it is off (or
    /// wedged). Raises no action — the radio is already in this state.
    pub fn set_bt_on(&mut self, on: bool) {
        self.bt_on = on;
    }

    /// Force the USB-DAC toggle to match reality, without raising a `UsbDacToggle` action.
    ///
    /// The toggle is our *intent*; the gadget's `sys.sony.config` is the *fact*, and they diverge
    /// whenever anything changes USB mode outside Cinder. The shell reconciles at startup by
    /// reading the property and calling this. It deliberately does NOT emit an action: the gadget
    /// is already in this state, so re-applying it would flip USB mode for real.
    pub fn set_usb_dac(&mut self, on: bool) {
        self.usb_dac_on = on;
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

    /// Brightness cycles 1..5, then 0 (backlight off), then wraps.
    ///
    /// This test used to assert that 0 was UNREACHABLE, because a persisted 0 would blank the
    /// panel and hide the Settings screen you need to undo it — across reboots. That hazard is
    /// real and has not gone away; what changed is how it is answered. Level 0 is now reachable
    /// but TRANSIENT: `brightness_restore` (never 0) is what gets persisted, and any input calls
    /// `brightness_wake`. See `backlight_off_is_reachable_and_always_escapable`, which pins those
    /// two properties — this one only pins the cycle order.
    #[test]
    fn brightness_cycles_one_to_five_then_backlight_off_then_wraps() {
        let mut app = unlocked();
        app.set_brightness(1);
        let mut seen = vec![app.brightness()];
        for _ in 0..6 {
            app.settings_sel = crate::settings::ROW_BRIGHTNESS;
            let acts = app.settings_activate();
            assert_eq!(acts, vec![Action::BrightnessChanged(app.brightness())]);
            seen.push(app.brightness());
        }
        assert!(seen.iter().all(|&l| l <= 5), "level left 0..=5: {seen:?}");
        // 1→2→3→4→5→0(off)→1 : every visible stop comes before the invisible one.
        assert_eq!(seen, vec![1, 2, 3, 4, 5, 0, 1]);
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

    /// Left-swipe queues NEXT, right-swipe queues LATER, and both report the change so the shell
    /// can schedule a flush. The two gestures are mirror images, so a mix-up would be invisible
    /// without the toast naming which one happened.
    #[test]
    fn swiping_a_row_queues_next_or_later() {
        let mut a = unlocked();
        a.stack = vec![Screen::Library];
        a.lib_tab = Tab::Songs;
        let y = crate::library::list_top(Tab::Songs) + 10;

        let acts = a.swipe(1, 240, y); // right = later
        assert_eq!(acts, vec![Action::QueueChanged]);
        assert_eq!(a.queue().len(), 1);
        assert!(a.toast.starts_with("Added to queue"), "toast was {:?}", a.toast);
        let later = a.queue()[0].title.clone();

        let acts = a.swipe(-1, 240, y + crate::library::row_h(Tab::Songs)); // left = next, a different row
        assert_eq!(acts, vec![Action::QueueChanged]);
        assert_eq!(a.queue().len(), 2);
        assert!(a.toast.starts_with("Playing next"), "toast was {:?}", a.toast);
        // "Next" must land in FRONT of what was already queued — that is the whole distinction.
        assert_eq!(a.queue()[1].title, later, "Play Next did not jump the queue");
    }

    /// Reordering and clearing both report a change, and neither may panic on a stale index — the
    /// queue can be edited while a gesture that started earlier is still in flight.
    #[test]
    fn queue_reorder_and_clear_are_safe() {
        let mut a = unlocked();
        a.stack = vec![Screen::Library];
        a.lib_tab = Tab::Songs;
        let y = crate::library::list_top(Tab::Songs) + 10;
        for i in 0..3 {
            a.swipe(1, 240, y + i * crate::library::row_h(Tab::Songs));
        }
        assert_eq!(a.queue().len(), 3);
        let first = a.queue()[0].title.clone();
        assert_eq!(a.queue_move(0, 2), vec![Action::QueueChanged]);
        assert_eq!(a.queue()[2].title, first, "the moved row did not land at its target");
        // Out-of-range and no-op moves change nothing and emit nothing.
        assert!(a.queue_move(9, 0).is_empty());
        assert!(a.queue_move(0, 9).is_empty());
        assert!(a.queue_move(1, 1).is_empty());
        assert_eq!(a.queue().len(), 3);
        assert_eq!(a.queue_clear(), vec![Action::QueueChanged]);
        assert!(a.queue().is_empty());
        assert!(a.queue_clear().is_empty(), "clearing an empty queue is not a change");
    }

    /// Build an app sitting on Up Next with `n` distinctly-titled tracks in the USER queue. Built
    /// directly rather than by swiping rows in, so the length isn't capped by the sample library
    /// (the edge-scroll test needs more tracks than fit on the panel). Swipe-to-queue itself is
    /// covered by `swiping_a_row_queues_next_or_later`.
    fn queued(n: usize) -> App {
        let mut a = unlocked();
        let src = a.lib.songs.first().cloned().expect("sample library has songs");
        for i in 0..n {
            let mut s = src.clone();
            s.title = format!("Q{i}");
            s.object_id = 9000 + i as i64;
            a.queue.push(s);
        }
        a.push(Screen::UpNext);
        a
    }
    /// Screen y of the middle of queue row `i` at the current scroll.
    /// Screen-y of the centre of user-queue row `i`. Reads the LAYOUT rather than assuming the
    /// queue starts at the top of the list — it no longer does: history, the playing track and a
    /// section heading can all sit above it.
    fn qrow_y(a: &App, i: usize) -> i32 {
        let top = a.up_next_layout().top_of(crate::up_next::Slot::Queued(i)).unwrap();
        crate::chrome::HEADER_BOTTOM + top + crate::up_next::RH / 2 - a.queue_scroll_px
    }

    /// The whole point: drag a queue row by its handle and it lands where you dropped it. Before
    /// this, `queue_move` existed and nothing could reach it.
    #[test]
    fn dragging_a_queue_row_by_its_handle_reorders_it() {
        let mut a = queued(4);
        let titles: Vec<String> = a.queue().iter().map(|s| s.title.clone()).collect();
        let grab = crate::up_next::GRIP_X0 + 20;
        assert!(a.reorder_begin(grab, qrow_y(&a, 0)), "the handle must pick the row up");
        assert_eq!(a.reorder_state().map(|d| (d.from, d.to)), Some((0, 0)));
        // Two rows down. `to` follows the row's CENTRE, so this is unambiguous.
        a.reorder_track(2 * crate::up_next::RH);
        assert_eq!(a.reorder_state().map(|d| d.to), Some(2));
        assert_eq!(a.reorder_release(), vec![Action::QueueChanged]);
        assert!(a.reorder_state().is_none(), "the drag must end at release");
        assert_eq!(a.queue()[2].title, titles[0], "the dragged row did not land at its target");
        assert_eq!(a.queue()[0].title, titles[1], "the rows below did not close up");
    }

    /// A drag that ends where it started is not a change — it must not spend a queue flush (which
    /// costs a SetTrackSequence at the next track boundary) on nothing.
    #[test]
    fn a_reorder_that_moves_nothing_reports_nothing() {
        let mut a = queued(3);
        assert!(a.reorder_begin(crate::up_next::GRIP_X0 + 20, qrow_y(&a, 1)));
        a.reorder_track(4); // less than half a row
        assert_eq!(a.reorder_state().map(|d| d.to), Some(1));
        assert!(a.reorder_release().is_empty());
    }

    /// Start-point ownership, the same rule the scrub rail uses. A drag that begins on the row body
    /// scrolls the list; only one that begins on the handle reorders. Without this, every attempt
    /// to scroll a long queue would pick a row up instead.
    #[test]
    fn only_the_grab_handle_starts_a_reorder() {
        let mut a = queued(4);
        assert!(!a.reorder_begin(40, qrow_y(&a, 0)), "the row body must still scroll");
        assert!(!a.reorder_begin(crate::up_next::GRIP_X0 - 1, qrow_y(&a, 0)));
        assert!(!a.reorder_begin(crate::up_next::GRIP_X0 + 20, 20), "the header is not a row");
        assert!(a.reorder_state().is_none());
        assert!(a.reorder_begin(crate::up_next::GRIP_X0 + 20, qrow_y(&a, 0)));
    }

    /// The album view of Up Next is the album's own track order, not ours to rewrite — and there is
    /// nothing to reorder on any other screen either.
    #[test]
    fn reorder_only_applies_to_the_user_queue() {
        let mut a = unlocked();
        a.push(Screen::UpNext);
        assert!(!a.reorder_begin(crate::up_next::GRIP_X0 + 20, 200), "empty user queue");
        let mut a = queued(3);
        a.go(Screen::Library);
        assert!(!a.reorder_begin(crate::up_next::GRIP_X0 + 20, 200), "wrong screen");
    }

    /// A tap on the handle must NOT play the track. A reorder that ends too short to classify as a
    /// drag arrives here as a tap, and "I nudged it and it started playing" is the worst outcome.
    #[test]
    fn tapping_the_grab_handle_does_not_play() {
        let mut a = queued(3);
        assert!(a.tap(crate::up_next::GRIP_X0 + 20, qrow_y(&a, 1)).is_empty());
        assert_eq!(a.current(), Screen::UpNext, "and it must not navigate either");
        assert!(!a.modal_open(), "nor may it raise the replace-the-queue prompt");
        // The row body still plays — and plays the QUEUE from that row, not the tapped track's
        // album. Playing the album is what made Up Next a display-only list: the screen showed one
        // sequence while the transport stepped through another.
        assert_eq!(a.tap(60, qrow_y(&a, 1)), vec![Action::PlayQueueAt(1)]);
    }

    /// Holding the row at the bottom edge scrolls the list under it, so a track can be moved
    /// further than one screenful. Driven from the tick, because a finger held still delivers no
    /// touch events at all — which is precisely when this has to keep working.
    #[test]
    fn holding_a_dragged_row_at_the_edge_scrolls_the_queue() {
        let mut a = queued(40);
        assert!(a.up_next_layout().max_scroll_px() > 0, "queue must overflow");
        assert!(a.reorder_begin(crate::up_next::GRIP_X0 + 20, qrow_y(&a, 0)));
        a.reorder_track(700); // park it against the bottom edge
        let before = a.queue_scroll_px;
        for _ in 0..10 {
            a.tick_dt(16);
        }
        assert!(a.queue_scroll_px > before, "the list did not scroll under the held row");
        assert!(a.reorder_state().map(|d| d.to).unwrap() > 0, "the landing slot must follow");
    }

    /// The EQ's raw band units are HALF-decibels, measured on device (`cinder-probe --eq`): the
    /// ladder -20..+20 came back unclamped with dB -10.0..+10.0. So the label has to halve the raw
    /// value — printing it straight claimed twice the boost the DSP applies — and the range has to
    /// be the full +/-20, not the +/-6 that reached less than a third of it.
    #[test]
    fn eq_band_labels_are_real_decibels_over_sonys_full_range() {
        assert_eq!(crate::eq::BAND_MAX, 20);
        assert_eq!(crate::eq::band_db(crate::eq::BAND_MAX), 10.0);
        assert_eq!(crate::eq::band_db(-crate::eq::BAND_MAX), -10.0);
        assert_eq!(crate::eq::band_db(0), 0.0);
        assert_eq!(crate::eq::band_db(crate::eq::BAND_STEP), 1.0, "one tap is one dB");
        // Every shipped preset must sit inside the measured range.
        for (name, bands) in data::EQ_PRESETS {
            for b in bands {
                assert!(b.abs() <= crate::eq::BAND_MAX, "{name} band {b} is out of range");
            }
        }
    }

    /// The bar is grabbable and the CONTENT follows the THUMB: dragging the thumb its full travel
    /// must cover the whole list, however long it is. A 1:1 finger-to-content mapping would need a
    /// 3400-row drag on the real library, which is the bug this replaces.
    #[test]
    fn dragging_the_scrollbar_scrolls_by_thumb_travel() {
        let mut a = unlocked();
        a.stack = vec![Screen::Library];
        a.lib_tab = Tab::Songs;
        let max = a.lib_max_scroll();
        assert!(max > 0, "the sample library must overflow the panel");
        let top = library::list_top(Tab::Songs);
        let x = crate::canvas::W as i32 - 4;
        assert!(a.sbar_begin(x, top + 20), "the right-edge strip must take a vertical drag");
        assert!(a.sbar_active());

        let span = library::sbar_span(top, max + (library::list_bottom() - top));
        assert!(span > 0);
        a.sbar_track(span / 2);
        let half = a.lib_scroll_px;
        assert!((half - max / 2).abs() <= 2, "half the thumb travel = half the list: {half} vs {}", max / 2);
        a.sbar_track(span);
        assert_eq!(a.lib_scroll_px, max, "full travel reaches the end");
        a.sbar_track(span * 4);
        assert_eq!(a.lib_scroll_px, max, "and clamps there");
        a.sbar_track(-span * 4);
        assert_eq!(a.lib_scroll_px, 0, "dragging back up returns to the top");
        a.sbar_release();
        assert!(!a.sbar_active());
    }

    /// The strip is shared with the A–Z rail and split by GESTURE. A TAP there must still jump to
    /// the letter — if the drag swallowed the strip, the rail would become undismissable dead space.
    #[test]
    fn the_scrollbar_strip_still_taps_through_to_the_az_rail() {
        let mut a = unlocked();
        a.stack = vec![Screen::Library];
        a.lib_tab = Tab::Songs;
        a.lib_sort = 0; // TITLE — the rail is shown
        let x = crate::canvas::W as i32 - 4;
        // Find a letter the library actually has, and tap it.
        let top = library::list_top(Tab::Songs);
        let hit = (top..library::list_bottom()).find(|&y| {
            library::az_letter_at(y, Tab::Songs)
                .and_then(|l| library::az_scroll_for(Tab::Songs, &a.lib, l, 0, 0, None))
                .is_some_and(|px| px > 0)
        });
        let y = hit.expect("some letter jumps somewhere");
        a.tap(x, y);
        assert!(a.lib_scroll_px > 0, "a tap on the strip must still be an A-Z jump");
        assert!(!a.sbar_active(), "and must not leave a drag armed");
    }

    /// Nothing to scroll = no grab, so the contact stays available to whatever is underneath
    /// instead of being silently eaten by an invisible bar.
    #[test]
    fn the_scrollbar_declines_when_there_is_nothing_to_scroll() {
        let mut a = unlocked();
        a.push(Screen::UpNext); // empty user queue -> no px scroll on this screen
        assert!(!a.sbar_begin(crate::canvas::W as i32 - 4, 300));
        assert!(!a.sbar_active());
    }

    /// The queue's grab handle and the scrollbar cannot both own the same pixels.
    #[test]
    fn the_queue_grip_stops_short_of_the_scrollbar_strip() {
        assert!(crate::up_next::queue_grip_hit(crate::up_next::GRIP_X0));
        assert!(crate::up_next::queue_grip_hit(crate::up_next::GRIP_X1 - 1));
        assert!(!crate::up_next::queue_grip_hit(crate::up_next::GRIP_X1));
        assert!(library::sbar_hit_x(crate::up_next::GRIP_X1), "the strips must abut, not overlap");
        assert!(!library::sbar_hit_x(crate::up_next::GRIP_X1 - 1));
    }

    /// Swiping a queue row removes it — either direction, because "queue this" makes no sense for
    /// a track that is already queued.
    #[test]
    fn swiping_a_queue_row_removes_it() {
        for dir in [-1, 1] {
            let mut a = queued(4);
            let y = qrow_y(&a, 1);
            assert!(a.swipe_track(dir * 80, y), "the row must take the gesture");
            assert_eq!(a.swipe(dir, 240, y), vec![Action::QueueChanged]);
            assert_eq!(a.queue().len(), 3);
            assert!(a.queue().iter().all(|s| s.title != "Q1"), "wrong row removed (dir {dir})");
        }
    }

    /// Emptying the queue cannot be undone, so it is an explicit labelled chip rather than a
    /// gesture — and the chip must not also play the row it sits over.
    #[test]
    fn the_clear_chip_empties_the_queue() {
        let mut a = queued(5);
        let (cx, cy, cw, ch) = crate::up_next::CLEAR_CHIP;
        assert_eq!(a.tap(cx + cw / 2, cy + ch / 2), vec![Action::QueueChanged]);
        assert!(a.queue().is_empty());
        assert_eq!(a.current(), Screen::UpNext);
        // And on an empty queue it is inert rather than emitting a no-op change.
        assert!(a.tap(cx + cw / 2, cy + ch / 2).is_empty());
    }

    /// Playing an album sets the CONTEXT; it does not queue anything. This test used to assert the
    /// opposite (`playback_fills_the_queue`) because the two were one list — which is why Up Next
    /// drew the album twice and a swipe-queued song had nothing to jump ahead of.
    #[test]
    fn playback_sets_the_context_not_the_queue() {
        let mut a = unlocked();
        let rows: Vec<SongRow> = (0..3)
            .map(|i| SongRow { title: format!("S{i}"), object_id: 10 + i, ..Default::default() })
            .collect();
        a.set_play_context(rows.clone(), 1);
        assert_eq!(a.context().len(), 3);
        assert_eq!(a.context_idx(), 1, "the track that started leads the context");
        assert!(a.queue().is_empty(), "playing an album queues nothing");

        // A second sequence REPLACES the first: it is what is playing, not an accumulation.
        a.set_play_context(rows.clone(), 0);
        assert_eq!(a.context().len(), 3);
        assert_eq!(a.context_idx(), 0);

        // And it does not arm the replace prompt — nothing here was hand-picked.
        assert_eq!(a.start_play(99), vec![Action::PlayIndex(99)]);
        assert!(!a.modal_open());

        // A start index past the end cannot point outside the list.
        a.set_play_context(rows, 99);
        assert_eq!(a.context_idx(), 2);
    }

    /// The order handed to the player: current track, then the USER'S picks, then the rest of the
    /// context. The middle term is the entire point of a queue.
    #[test]
    fn a_queued_song_plays_before_the_rest_of_the_album() {
        let mut a = unlocked();
        let album: Vec<SongRow> = (0..4)
            .map(|i| SongRow { title: format!("A{i}"), object_id: 10 + i, ..Default::default() })
            .collect();
        a.set_play_context(album, 1);                       // A1 playing
        a.queue.push(SongRow { title: "PICK".into(), object_id: 99, ..Default::default() });
        // What Up Next draws, in order, is what the shell then builds the sequence from.
        let l = a.up_next_layout();
        let order: Vec<crate::up_next::Slot> = l.slots.iter().map(|(s, _)| *s).collect();
        use crate::up_next::{Section, Slot};
        assert_eq!(order, vec![
            Slot::Head(Section::History), Slot::History(0),
            Slot::Head(Section::Now),     Slot::Current(1),
            Slot::Head(Section::Queue),   Slot::Queued(0),
            Slot::Head(Section::Album),   Slot::Upcoming(2), Slot::Upcoming(3),
        ], "the pick sits between the playing track and the rest of the album");
    }

    /// MIX shuffles what is still to come and nothing else: the played tracks keep their order
    /// (rewriting history helps nobody) and the user's own picks keep theirs (they chose it).
    #[test]
    fn queue_shuffle_only_moves_what_is_still_to_come() {
        let mut a = unlocked();
        let album: Vec<SongRow> = (0..12)
            .map(|i| SongRow { title: format!("A{i}"), object_id: 10 + i, ..Default::default() })
            .collect();
        a.set_play_context(album.clone(), 2);
        a.queue.push(SongRow { title: "PICK".into(), object_id: 99, ..Default::default() });
        let acts = a.queue_shuffle();
        assert_eq!(acts, vec![Action::QueueChanged], "the sequence must be re-issued");
        // History + current untouched.
        for i in 0..=2 {
            assert_eq!(a.context()[i].object_id, album[i].object_id, "row {i} moved");
        }
        // The tail is a permutation of what it was, not a truncation or a duplication.
        let mut before: Vec<i64> = album[3..].iter().map(|t| t.object_id).collect();
        let mut after: Vec<i64> = a.context()[3..].iter().map(|t| t.object_id).collect();
        before.sort_unstable();
        after.sort_unstable();
        assert_eq!(before, after);
        assert_ne!(
            a.context()[3..].iter().map(|t| t.object_id).collect::<Vec<_>>(),
            album[3..].iter().map(|t| t.object_id).collect::<Vec<_>>(),
            "12 tracks should not shuffle back into their original order"
        );
        // The user's picks are theirs.
        assert_eq!(a.queue().len(), 1);
        assert_eq!(a.queue()[0].object_id, 99);
        // Nothing left to shuffle => no action, no crash.
        a.set_play_context(vec![SongRow::default()], 0);
        assert!(a.queue_shuffle().is_empty());
    }

    /// The context index follows playback, so Up Next's history/up-next split stays true.
    #[test]
    fn the_context_index_follows_the_track_that_starts() {
        let mut a = unlocked();
        let album: Vec<SongRow> = (0..3)
            .map(|i| SongRow { title: format!("A{i}"), object_id: 10 + i, ..Default::default() })
            .collect();
        a.set_play_context(album, 0);
        assert!(a.set_context_playing(12));
        assert_eq!(a.context_idx(), 2);
        assert!(!a.set_context_playing(12), "no move, no repaint");
        // A track that is not in the context (a user-queue pick) leaves the index alone rather
        // than guessing where we are.
        assert!(!a.set_context_playing(9999));
        assert_eq!(a.context_idx(), 2);
    }

    /// "Keep queue" has to keep it: starting something else clears the user's picks unless they
    /// said not to.
    #[test]
    fn keeping_the_queue_appends_the_new_sequence() {
        use crate::confirm::{hit, Ask, Hit};
        let pick = |want: Hit| {
            (0..crate::canvas::H as i32)
                .find(|y| hit(Ask::QueueOnPlay, 240, *y) == want)
                .expect("row has no tappable pixel")
        };
        let mut a = unlocked();
        a.queue_push_for_test(); // one hand-swiped pick
        assert!(a.start_play(77).is_empty(), "hand-built picks still prompt");
        assert_eq!(a.tap(240, pick(Hit::KeepQueue)), vec![Action::PlayIndex(77)]);

        let rows: Vec<SongRow> = (0..2)
            .map(|i| SongRow { title: format!("S{i}"), object_id: 10 + i, ..Default::default() })
            .collect();
        a.set_play_context(rows, 0);
        assert_eq!(a.queue().len(), 1, "the pick must survive");
        assert_eq!(a.context().len(), 2, "and the new sequence is the context");
        // The flag is one-shot: the next play clears the picks.
        a.set_play_context(vec![SongRow::default()], 0);
        assert!(a.queue().is_empty());
    }

    /// The user queue used to draw from row 0 and stop at the panel edge, so past ~10 tracks it was
    /// both unreachable and unreorderable.
    #[test]
    fn the_user_queue_scrolls() {
        let mut a = queued(40);
        let max = a.up_next_layout().max_scroll_px();
        assert!(max > 0);
        a.scroll_px(10_000);
        assert_eq!(a.queue_scroll_px, max, "must scroll, and clamp to the end");
        // And the hit test follows the scroll rather than always answering row 0. Row 0's screen
        // position at scroll 0 must now resolve to a LATER row (or off the list) once scrolled.
        let y_unscrolled = crate::chrome::HEADER_BOTTOM
            + a.up_next_layout().top_of(crate::up_next::Slot::Queued(0)).unwrap()
            + crate::up_next::RH / 2;
        assert_ne!(
            a.up_next_layout().at(y_unscrolled, a.queue_scroll_px),
            Some(crate::up_next::Slot::Queued(0))
        );
    }

    /// Repeat is two states, both of which do something. It used to cycle off → all → one and tell
    /// PlayerService nothing at all; "all" has no known primitive, so a third position would still
    /// be decorative. Every press must also emit an action, or the shell never applies it.
    #[test]
    fn repeat_is_two_real_states_and_always_reaches_the_shell() {
        let mut a = unlocked();
        assert_eq!(a.current(), Screen::NowPlaying);
        // The repeat icon lives at the far right of the transport row.
        let acts = a.tap(436, 692);
        assert_eq!(acts, vec![Action::RepeatCycle], "tapping repeat must emit an action");
        let acts = a.tap(436, 692);
        assert_eq!(acts, vec![Action::RepeatCycle], "and again on the way back off");
    }

    /// Shuffle likewise: the icon is at the far LEFT of the same row, and must not be swallowed by
    /// the prev-track target beside it.
    #[test]
    fn shuffle_and_repeat_do_not_steal_each_others_taps() {
        let mut a = unlocked();
        assert_eq!(a.tap(44, 692), vec![Action::ShuffleToggle]);
        assert_eq!(a.tap(436, 692), vec![Action::RepeatCycle]);
        assert_eq!(a.tap(130, 692), vec![Action::Prev]);
        assert_eq!(a.tap(350, 692), vec![Action::Next]);
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
                let Some(px) = library::az_scroll_for(
                    tab, &app.lib, letter, app.lib_sort, app.album_sort, app.album_expanded,
                ) else {
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
            assert_eq!(a.eq_bands[band], crate::eq::BAND_STEP,
                       "band {band} above the line should raise");
            assert_eq!(a.eq_sel, band, "tap should select the band under the finger");
            a.tap(x, (FIELD_MID + FIELD_BOTTOM) / 2); // lower half
            assert_eq!(a.eq_bands[band], 0, "band {band} below the line should lower");
        }
        // Just below the zero line lowers (this pixel used to raise).
        a.tap(band_center_x(3), FIELD_MID + 1);
        assert_eq!(a.eq_bands[3], -crate::eq::BAND_STEP);
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
        let mut a = two_playlists();
        assert_eq!(a.press(Button::Select), vec![Action::PlayPlaylist(77)]);

        // Same for a tap on the ROW BUTTON (x >= 404). Derive the y from the render's own geometry
        // (list_top + row_h) rather than a magic number, so this can't drift from the layout.
        let y = library::list_top(Tab::Playlists) + library::row_h(Tab::Playlists) + 4;
        assert_eq!(a.tap(434, y), vec![Action::PlayPlaylist(78)]);
        assert_eq!(a.lib_index(), 1);
    }

    /// Two playlists with real members, sitting on the Playlists tab.
    fn two_playlists() -> App {
        let mut a = unlocked();
        a.push(Screen::Library);
        a.lib_tab = Tab::Playlists;
        a.lib_idx = 0;
        let songs: Vec<SongRow> = (0..5)
            .map(|i| SongRow {
                title: format!("P{i}"), artist: "Someone".into(), dur: "3:00".into(),
                object_id: 500 + i, ..Default::default()
            })
            .collect();
        a.lib = crate::model::Library {
            playlists: vec![
                crate::model::PlaylistRow {
                    id: 77, name: "Night Bus".into(), tracks: 3, art: "Night Bus".into(),
                    track_list: songs[..3].to_vec(),
                },
                crate::model::PlaylistRow {
                    id: 78, name: "Morning".into(), tracks: 2, art: "Morning".into(),
                    track_list: songs[3..].to_vec(),
                },
            ],
            ..Default::default()
        };
        a
    }

    /// Tapping the row body opens that playlist's own page — the same shape the Artists tab has.
    /// The row used to be a shortcut that played the whole list, which left no way to see what was
    /// in one before committing to it.
    #[test]
    fn tapping_a_playlist_row_opens_its_page() {
        let mut a = two_playlists();
        let y = library::list_top(Tab::Playlists) + library::row_h(Tab::Playlists) + 4;
        assert!(a.tap(120, y).is_empty(), "opening a page emits no action");
        assert_eq!(a.current(), Screen::Playlist);
        assert_eq!(a.playlist_row().map(|p| p.id), Some(78));

        // A track row plays; the band shuffles that playlist by id.
        let ty = library::playlist_content_top() + library::PLAYLIST_TRACK_RH / 2;
        assert_eq!(a.tap(200, ty), vec![Action::PlayIndex(503)]);
        assert_eq!(a.current(), Screen::Playlist, "playing must not leave the page");
        let (bx, by, _, bh) = library::shuffle_band_rect(library::PLAYLIST_BAND_Y);
        assert_eq!(a.tap(bx + 20, by + bh / 2), vec![Action::ShufflePlaylist(78)]);

        // And Back returns to the tab.
        a.press(Button::Back);
        assert_eq!(a.current(), Screen::Library);
    }

    /// The page scrolls, and a right-swipe on one of its rows queues that track like every other
    /// track list in the app.
    #[test]
    fn the_playlist_page_scrolls_and_queues() {
        let mut a = two_playlists();
        a.open_playlist(0);
        let ty = library::playlist_content_top() + library::PLAYLIST_TRACK_RH / 2;
        assert_eq!(a.swipe(1, 240, ty), vec![Action::QueueChanged]);
        assert_eq!(a.queue().len(), 1);
        assert_eq!(a.queue()[0].title, "P0");
        // Three rows do not overflow the panel, so there is nothing to scroll — and the scrollbar
        // must decline rather than eat the contact.
        assert_eq!(library::playlist_max_scroll_px(a.playlist_row().unwrap()), 0);
        assert!(!a.sbar_begin(crate::canvas::W as i32 - 4, ty));
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
        assert_eq!(a.eq_bands[1], (before + crate::eq::BAND_STEP).min(crate::eq::BAND_MAX));
        assert!(matches!(acts.as_slice(), [Action::EqChanged(_)]));
        // Clamps symmetrically at the limit the slider field is DRAWN to — a knob that stops at
        // 60% of its column is the bug this replaces.
        for _ in 0..40 {
            a.press(Button::Up);
        }
        assert_eq!(a.eq_bands[1], crate::eq::BAND_MAX);
        for _ in 0..40 {
            a.press(Button::Down);
        }
        assert_eq!(a.eq_bands[1], -crate::eq::BAND_MAX);
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

    /// Sony's "Use Enhanced Mode" (firmware message 230077) is the AVRCP absolute-volume switch.
    /// It defaults ON because the OFF path sends VOLUME_UP/VOLUME_DOWN key events, which sinks
    /// like the CMF Buds answer with their own feedback beep — and because Sony's own
    /// SetCurrentVolume refuses to transmit while the preference is clear.
    /// Pinning from the Albums accordion names the OPEN ALBUM, and restoring jumps to it —
    /// recomputing the scroll from the album's current position rather than trusting a pixel
    /// offset that a library rebuild would have invalidated.
    #[test]
    fn albums_pin_names_the_open_album_and_restores_as_a_jump_to() {
        let mut a = unlocked();
        a.go_for_preview(Screen::Library);
        a.lib_tab = Tab::Albums;
        a.album_expanded = None;
        // With nothing expanded the pin is just "the Albums tab".
        let (t, _) = a.place_label();
        assert_eq!(t, "Library");
        // Expand one, and the pin takes its name.
        let flat = 1usize;
        a.album_expanded = Some(flat);
        let want = a.lib.albums_flat().get(flat).map(|al| al.name.clone()).unwrap();
        let (t, sub) = a.place_label();
        assert_eq!(t, want);
        assert_eq!(sub, a.lib.albums_flat()[flat].artist.clone());
        // Pin it, scroll somewhere else, restore: the album comes back expanded and in view.
        let pin = a.capture_pin();
        a.lib_scroll_px = 4000;
        a.album_expanded = None;
        a.restore_pin(&pin);
        assert_eq!(a.album_expanded, Some(flat));
        assert_eq!(a.lib_tab, Tab::Albums);
        let rank = a.album_rank_of(flat).unwrap();
        let expect = crate::library::row_top_px(
            Tab::Albums, &a.lib, rank, a.album_sort, a.album_expanded);
        assert_eq!(a.lib_scroll_px, expect.clamp(0, a.lib_max_scroll()));
    }

    #[test]
    fn bluetooth_enhanced_mode_toggles_and_is_inert_while_off() {
        let mut a = enter_bluetooth();
        assert!(a.bt_enhanced(), "enhanced mode defaults on");
        // 576 is inside the row (556..620) and clear of the LDAC chips above and Pair below.
        assert_eq!(a.tap(240, 576), vec![Action::BtEnhancedChanged]);
        assert!(!a.bt_enhanced());
        assert_eq!(a.tap(240, 576), vec![Action::BtEnhancedChanged]);
        assert!(a.bt_enhanced());
        // Radio off ⇒ the whole panel below the header is inert, this row included.
        a.tap(430, 64); // header toggle
        assert!(!a.bt_on);
        assert!(a.tap(240, 576).is_empty());
        assert!(a.bt_enhanced(), "an inert tap must not flip the preference");
    }

    /// Support is a fact from the radio, not a preference: it only repaints when it CHANGES, so
    /// the shell's per-connect push can't dirty a frame on every poll.
    #[test]
    fn bluetooth_enhanced_supported_reports_change_only() {
        let mut a = enter_bluetooth();
        assert!(!a.set_bt_enhanced_supported(true), "already true");
        assert!(a.set_bt_enhanced_supported(false));
        assert!(!a.set_bt_enhanced_supported(false));
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
        assert_eq!(a.swipe(1, 240, 220), vec![Action::QueueChanged]);
        assert_eq!(a.queue().len(), 1);
        let expected = library::song_at(&a.lib, a.lib_sort, 0).unwrap().title.clone();
        assert_eq!(a.queue()[0].title, expected);
        // Feedback started: toast + row chip animation, and tick() reports animation frames.
        assert!(a.toast.starts_with("Added to queue"));
        assert_eq!(a.queue_anim_frames, QUEUE_ANIM_FRAMES);
        assert_eq!(a.queue_anim_y, 220);
        assert!(a.tick());
        // A LEFTWARD swipe now queues too — as PLAY NEXT, in front of what is already there.
        // It used to do nothing at all; the gesture was free and is now the mirror of the right.
        assert_eq!(a.swipe(-1, 240, 220), vec![Action::QueueChanged]);
        assert_eq!(a.queue().len(), 2);
        assert!(a.toast.starts_with("Playing next"), "toast was {:?}", a.toast);
        // A rightward swipe on chrome (above the rows) queues nothing.
        assert!(a.swipe(1, 240, 100).is_empty());
        assert_eq!(a.queue().len(), 2);
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
        // Queuing now reports the change so the shell can schedule a boundary flush.
        assert_eq!(a.swipe(1, 240, y), vec![Action::QueueChanged]);
        assert_eq!(a.queue().len(), 1);
        assert_eq!(a.queue()[0].object_id, want.object_id);
        // Collapsed, that same y is no longer a track row -> nothing queued.
        a.album_expanded = None;
        assert!(a.swipe(1, 240, y).is_empty(), "a non-row swipe must queue nothing");
        assert_eq!(a.queue().len(), 1);
    }

    /// Tapping a row plays that track. The layout is what `tap` resolves against, so a test drives
    /// it exactly as `render` does — build the layout, publish it and the album ids, then tap.
    #[test]
    fn up_next_row_tap_plays_that_track() {
        let mut a = unlocked();
        a.push(Screen::UpNext);
        // Three album tracks, the middle one playing: HISTORY hdr, 101, NOW hdr, 202, NEXT hdr, 303.
        a.set_play_context(
            [101i64, 202, 303].iter()
                .map(|&object_id| SongRow { object_id, ..Default::default() })
                .collect(),
            1,
        );
        a.queue_scroll_px = 0;
        let l = a.up_next_layout();
        let top = crate::chrome::HEADER_BOTTOM;
        let y_of = |slot| top + l.top_of(slot).unwrap() + crate::up_next::RH / 2;
        // A history row plays that track and re-arms the follow.
        a.queue_follow = false;
        assert_eq!(a.tap(240, y_of(crate::up_next::Slot::History(0))), vec![Action::PlayIndex(101)]);
        assert!(a.queue_follow, "playing from the list re-arms the auto-follow");
        assert_eq!(a.current(), Screen::UpNext, "playing a row should not leave the screen");
        // An upcoming row does the same.
        assert_eq!(a.tap(240, y_of(crate::up_next::Slot::Upcoming(2))), vec![Action::PlayIndex(303)]);
        // The NOW PLAYING row is where you already are — it just opens Now Playing.
        assert!(a.tap(240, y_of(crate::up_next::Slot::Current(1))).is_empty());
        assert_eq!(a.current(), Screen::NowPlaying);
    }

    /// The Apple Music order, and the fact that empty sections vanish entirely.
    #[test]
    fn up_next_layout_is_history_then_now_then_queue_then_album() {
        use crate::up_next::{layout, Section, Slot};
        let l = layout(4, Some(1), 2);
        let kinds: Vec<Slot> = l.slots.iter().map(|(s, _)| *s).collect();
        assert_eq!(
            kinds,
            vec![
                Slot::Head(Section::History),
                Slot::History(0),
                Slot::Head(Section::Now),
                Slot::Current(1),
                Slot::Head(Section::Queue),
                Slot::Queued(0),
                Slot::Queued(1),
                Slot::Head(Section::Album),
                Slot::Upcoming(2),
                Slot::Upcoming(3),
            ]
        );
        // Track 0 playing => no history section at all, header included.
        let l = layout(2, Some(0), 0);
        assert_eq!(
            l.slots.iter().map(|(s, _)| *s).collect::<Vec<_>>(),
            vec![Slot::Head(Section::Now), Slot::Current(0), Slot::Head(Section::Album), Slot::Upcoming(1)]
        );
        // Nothing playing, but a user queue => just the queue.
        let l = layout(0, None, 1);
        assert_eq!(
            l.slots.iter().map(|(s, _)| *s).collect::<Vec<_>>(),
            vec![Slot::Head(Section::Queue), Slot::Queued(0)]
        );
        // Nothing at all => the empty state, and no scroll.
        let l = layout(0, None, 0);
        assert!(l.slots.is_empty());
        assert_eq!(l.max_scroll_px(), 0);
    }

    /// The queue follows playback until the user scrolls, and re-entering the screen re-arms it.
    #[test]
    fn up_next_follows_playback_until_the_user_scrolls() {
        let mut a = unlocked();
        a.push(Screen::UpNext);
        assert!(a.queue_follow, "arriving on the screen always shows the current track");
        // A long album so there is somewhere to scroll to.
        a.set_play_context((0..40).map(|object_id| SongRow { object_id, ..Default::default() }).collect(), 20);
        a.queue_scroll_px = a.up_next_layout().follow_scroll();
        assert!(a.queue_scroll_px > 0, "row 20 of 40 must be scrolled to");
        a.scroll_px(-200); // drag the list
        assert!(!a.queue_follow, "a deliberate scroll takes the list over");
        // Leaving and coming back resets it.
        a.pop();
        a.push(Screen::UpNext);
        assert!(a.queue_follow);
        assert_eq!(a.up_next_cur, None);
    }

    /// Playing something from ANYWHERE re-arms the follow. Found in the queue bug sweep: the
    /// shell calls `set_play_queue` for every play that does not start on this screen, and it
    /// reset the scroll without re-arming — so one scroll of Up Next left it pinned to the top for
    /// the rest of the session, showing the wrong album's first track.
    #[test]
    fn playing_from_elsewhere_re_arms_the_queue_follow() {
        let mut a = unlocked();
        a.push(Screen::UpNext);
        // Something long enough to actually scroll — a scroll that cannot move the list is not a
        // takeover, and must NOT disarm the follow.
        a.set_play_context(
            (0..40).map(|object_id| SongRow { object_id, ..Default::default() }).collect(), 20);
        a.queue_scroll_px = a.up_next_layout().follow_scroll();
        a.scroll_px(-200);
        assert!(!a.queue_follow, "a scroll that moves the list hands it to the user");
        a.set_play_context(vec![SongRow::default(), SongRow::default()], 0);
        assert!(a.queue_follow, "a new play must make the queue follow again");
        assert_eq!(a.up_next_cur, None, "and the next render must treat it as a track change");
    }

    /// The BT scale went 30 -> 64 on 2026-08-11 so a press moves the sink ~2 AVRCP units instead
    /// of ~4.2. The HUD stretches whatever the scale is back over 0..VOL_MAX, so the bar must look
    /// the same at both ends however many steps there are.
    #[test]
    fn bt_volume_scale_is_finer_and_still_maps_onto_the_shared_hud() {
        use crate::overlay::{BT_VOL_MAX, BT_VOL_MAX_LEGACY, VOL_MAX};
        assert!(BT_VOL_MAX > BT_VOL_MAX_LEGACY, "the whole point is finer steps");
        // AVRCP is 0..127, so a step is now under 2.5 of its units.
        assert!(127 / BT_VOL_MAX as u32 <= 2);
        let mut a = unlocked();
        a.set_bt_route(true);
        a.set_bt_volume(0);
        assert_eq!(a.display_volume(), 0, "empty bar at the bottom");
        a.set_bt_volume(BT_VOL_MAX);
        assert_eq!(a.display_volume(), VOL_MAX, "full bar at the top");
        // And the level itself cannot exceed the scale.
        a.set_bt_volume(255);
        assert_eq!(a.bt_volume_level(), BT_VOL_MAX);
    }

    /// The scrollbar on this screen is `library::scrollbar`, whose drag maths measure against the
    /// LIBRARY's list bottom — so the two bottoms must be the same number, not two literals that
    /// happen to agree.
    #[test]
    fn up_next_list_bottom_matches_the_library_it_shares_a_scrollbar_with() {
        assert_eq!(
            crate::up_next::queue_view_h(),
            library::list_bottom() - crate::chrome::HEADER_BOTTOM
        );
    }

    /// The playing row parks a third of the way down, not pinned to the top — so the tracks you
    /// just heard stay visible above it.
    #[test]
    fn up_next_follow_parks_the_current_row_a_third_down() {
        let l = crate::up_next::layout(40, Some(20), 0);
        let view = crate::up_next::queue_view_h();
        let top = l.current_top.unwrap();
        assert_eq!(l.follow_scroll(), (top - view / 3).clamp(0, l.max_scroll_px()));
        // Early tracks cannot scroll above the start.
        assert_eq!(crate::up_next::layout(40, Some(0), 0).follow_scroll(), 0);
    }

    /// An empty queue keeps the old behaviour: a tap just leaves.
    #[test]
    fn up_next_tap_with_nothing_queued_returns_to_now_playing() {
        let mut a = unlocked();
        a.push(Screen::UpNext);
        a.set_play_context(Vec::new(), 0);
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

    // ── Power button hold -> Power menu ───────────────────────────────────────────────────────

    /// Holding Power opens the menu, and each row emits its own action. The whole gesture exists
    /// because acting on the PRESS meant a hold could only ever blank the screen.
    #[test]
    fn holding_power_opens_a_menu_whose_rows_act() {
        use crate::confirm::{hit, Ask, Hit};
        for (want, act) in [(Hit::PowerOff, Action::PowerOff), (Hit::Restart, Action::Restart)] {
            let mut a = unlocked();
            assert!(a.power_held(), "the hold should open the menu");
            assert!(a.modal_open());
            // Find a pixel that really is on the row we mean, then tap exactly there.
            let (x, y) = (0..crate::canvas::H as i32)
                .find_map(|y| (hit(Ask::PowerMenu, 240, y) == want).then_some((240, y)))
                .expect("row has no tappable pixel");
            assert_eq!(a.tap(x, y), vec![act]);
            assert!(!a.modal_open(), "answering must close the modal");
        }
    }

    /// Cancel — and the dimmed backdrop — must do nothing at all. A power menu that can be
    /// dismissed into a power-off is the one failure this whole widget exists to prevent.
    #[test]
    fn cancelling_the_power_menu_does_nothing() {
        use crate::confirm::{hit, Ask, Hit};
        let cancel_y = (0..crate::canvas::H as i32)
            .find(|y| hit(Ask::PowerMenu, 240, *y) == Hit::Cancel && *y > 400)
            .expect("no Cancel row");
        for (x, y) in [(240, cancel_y), (5, 5), (240, 60)] {
            let mut a = unlocked();
            assert!(a.power_held());
            assert!(a.tap(x, y).is_empty(), "({x},{y}) must not act");
            assert!(!a.modal_open(), "the tap must still dismiss");
        }
    }

    /// Hold engaged = the device is in a pocket. Nothing may open a power menu in there, and a
    /// refused hold must not leave a modal behind.
    #[test]
    fn a_locked_device_refuses_the_power_menu() {
        let mut a = unlocked();
        a.set_hold(true);
        assert!(!a.power_held());
        assert!(!a.modal_open());
    }

    /// A second hold must not stack or silently replace the question already on screen.
    #[test]
    fn a_second_hold_does_not_restack_the_menu() {
        let mut a = unlocked();
        assert!(a.power_held());
        assert!(!a.power_held(), "the menu is already up");
        assert!(a.modal_open());
    }

    /// Back is the second escape from a modal, and every other button is swallowed — a transport
    /// press must not reach the music underneath a "Power off?" prompt.
    #[test]
    fn back_dismisses_a_modal_and_other_buttons_are_swallowed() {
        let mut a = unlocked();
        assert!(a.power_held());
        for b in [Button::Play, Button::Next, Button::Prev, Button::Select, Button::Up] {
            assert!(a.press(b).is_empty(), "{b:?} leaked through the modal");
            assert!(a.modal_open(), "{b:?} should not have closed it");
        }
        assert!(a.press(Button::Back).is_empty());
        assert!(!a.modal_open(), "Back must dismiss");
    }

    /// The Settings rows still raise their own two-button confirms — the menu is an addition, not
    /// a replacement, and each card must answer for its own question.
    #[test]
    fn settings_rows_still_confirm_their_own_action() {
        use crate::confirm::{hit, Ask, Hit};
        for (row, ask, act) in [
            (crate::settings::ROW_RESTART, Ask::Restart, Action::Restart),
            (crate::settings::ROW_POWER_OFF, Ask::PowerOff, Action::PowerOff),
        ] {
            let mut a = unlocked();
            a.go(Screen::Settings);
            a.settings_sel = row;
            assert!(a.press(Button::Select).is_empty(), "the row itself must not act");
            assert!(a.modal_open());
            let (x, y) = (0..crate::canvas::H as i32)
                .find_map(|y| (hit(ask, 240 + 120, y) == Hit::Confirm).then_some((360, y)))
                .expect("no confirm pixel");
            assert_eq!(a.tap(x, y), vec![act]);
        }
    }

    // ── Playing a song while a hand-built queue exists ────────────────────────────────────────

    /// With an empty queue, playing is immediate — the prompt must not appear for the common case.
    #[test]
    fn an_empty_queue_plays_without_asking() {
        let mut a = unlocked();
        assert_eq!(a.start_play(77), vec![Action::PlayIndex(77)]);
        assert!(!a.modal_open());
    }

    /// With tracks queued it asks first, and each answer does exactly one thing.
    #[test]
    fn a_queued_song_asks_before_replacing_the_queue() {
        use crate::confirm::{hit, Ask, Hit};
        let pick = |want: Hit| {
            (0..crate::canvas::H as i32)
                .find(|y| hit(Ask::QueueOnPlay, 240, *y) == want)
                .expect("row has no tappable pixel")
        };

        // CLEAR: queue emptied, and the shell is told both things that changed.
        let mut a = unlocked();
        a.queue_push_for_test();
        assert!(a.start_play(77).is_empty(), "the tap must not play until answered");
        assert!(a.modal_open());
        let acts = a.tap(240, pick(Hit::ClearQueue));
        assert_eq!(acts, vec![Action::QueueChanged, Action::PlayIndex(77)]);
        assert!(a.queue().is_empty());

        // KEEP: plays, queue untouched, and no spurious QueueChanged.
        let mut a = unlocked();
        a.queue_push_for_test();
        let before = a.queue().len();
        assert!(a.start_play(77).is_empty());
        assert_eq!(a.tap(240, pick(Hit::KeepQueue)), vec![Action::PlayIndex(77)]);
        assert_eq!(a.queue().len(), before, "keeping must not touch the queue");

        // CANCEL: nothing plays, nothing changes, and the pending song is dropped so it cannot
        // leak into the NEXT prompt.
        let mut a = unlocked();
        a.queue_push_for_test();
        assert!(a.start_play(77).is_empty());
        assert!(a.tap(5, 5).is_empty());
        assert!(!a.modal_open());
        assert_eq!(a.queue().len(), 1);
        assert!(a.pending_song.is_none(), "a cancelled song must not survive the dialog");
    }

    /// Open the Library on a given tab, the way a user does.
    fn library_on(tab: Tab) -> App {
        let mut a = unlocked();
        a.go(Screen::Library);
        a.lib_tab = tab;
        a
    }

    /// The y of Artists row `i`, from the same geometry the renderer uses.
    fn artist_row_y(i: usize) -> i32 {
        library::list_top(Tab::Artists) + library::row_h(Tab::Artists) * i as i32 + 20
    }

    #[test]
    fn an_artist_row_opens_that_artists_page_and_back_returns_to_the_list() {
        let mut a = library_on(Tab::Artists);
        assert!(a.lib.artists.len() > 1, "sample library must have artists to open");
        // Tap the SECOND row — row 0 would also be the default index, so it could pass by accident.
        assert!(a.tap(200, artist_row_y(1)).is_empty(), "opening a page plays nothing");
        assert_eq!(a.current(), Screen::Artist);
        assert_eq!(a.artist_view, 1);
        // The page is the one that row named.
        let page = a.artist_page().expect("page resolves");
        assert_eq!(page.name, a.lib.artists[1].name);
        // Back returns to the list, still on Artists.
        assert_eq!(a.press(Button::Back), vec![]);
        assert_eq!(a.current(), Screen::Library);
        assert_eq!(a.lib_tab, Tab::Artists);
    }

    #[test]
    fn the_row_shuffle_button_shuffles_that_artist_without_opening_the_page() {
        let mut a = library_on(Tab::Artists);
        // The button is drawn at x 414..454; anywhere in that block is it.
        assert_eq!(a.tap(434, artist_row_y(1)), vec![Action::ShuffleArtist(1)]);
        assert_eq!(a.current(), Screen::Library, "the button must not also navigate");
        // And the band on the page itself shuffles the artist whose page it is.
        a.tap(200, artist_row_y(1));
        let (bx, by, _, bh) = library::shuffle_band_rect(library::ARTIST_BAND_Y);
        assert_eq!(a.tap(bx + 20, by + bh / 2), vec![Action::ShuffleArtist(1)]);
    }

    #[test]
    fn an_artist_page_track_row_plays_and_swipes_to_the_queue() {
        let mut a = library_on(Tab::Artists);
        a.tap(200, artist_row_y(0));
        assert_eq!(a.current(), Screen::Artist);
        // The first track row, located through the page layout rather than a literal.
        let (first_track_y, want) = {
            let p = a.artist_page().expect("page");
            let vy = p
                .rows
                .iter()
                .find_map(|(vy, r)| matches!(*r, library::ArtistRowKind::Song(0)).then_some(*vy))
                .expect("the artist has tracks");
            (library::artist_content_top() + vy + 4, p.tracks[0].song.object_id)
        };
        assert_eq!(a.tap(200, first_track_y), vec![Action::PlayIndex(want)]);
        // Right-swiping the same row queues it instead (empty queue → no prompt involved).
        let mut a = library_on(Tab::Artists);
        a.tap(200, artist_row_y(0));
        assert!(a.queue().is_empty());
        a.swipe(1, 200, first_track_y);
        assert_eq!(a.queue().len(), 1, "a swipe on an artist-page track must queue it");
    }

    #[test]
    fn only_rows_with_a_track_under_the_finger_move_with_the_swipe() {
        // A song row takes the gesture …
        let mut a = library_on(Tab::Songs);
        let song_y = library::list_top(Tab::Songs) + library::row_h(Tab::Songs) + 20;
        assert!(a.swipe_track(40, song_y));
        assert_eq!(a.swipe_state().map(|s| s.dx), Some(40));
        // … an ARTIST row does not: there is nothing to queue, so a row that slid would be
        // promising an action release cannot perform.
        let mut a = library_on(Tab::Artists);
        assert!(!a.swipe_track(40, artist_row_y(1)));
        assert_eq!(a.swipe_state(), None);
        // Neither does the shuffle band above the list.
        let mut a = library_on(Tab::Songs);
        let (_, by, _, bh) = library::library_shuffle_band();
        assert!(!a.swipe_track(40, by + bh / 2));
        assert_eq!(a.swipe_state(), None);
    }

    #[test]
    fn a_released_row_animates_back_to_rest_and_stops() {
        let mut a = library_on(Tab::Songs);
        let song_y = library::list_top(Tab::Songs) + library::row_h(Tab::Songs) + 20;
        a.swipe_track(200, song_y);
        let held = a.swipe_state().expect("row is travelling").dx;
        assert!(held > 0);
        // While the finger is down the row holds its offset, however many frames pass.
        for _ in 0..60 {
            a.tick();
        }
        assert_eq!(a.swipe_state().map(|s| s.dx), Some(held), "a held row must not drift home");
        // After release it decays to rest — and then STOPS asking for frames, or the device would
        // repaint forever for an animation that has finished.
        a.swipe_release();
        let mut frames = 0;
        while a.swipe_state().is_some() {
            assert!(a.tick(), "an animating row must report that it needs a repaint");
            frames += 1;
            assert!(frames < 240, "the snap-back never settled");
        }
        assert!(frames > 1, "the row must animate home, not teleport");
    }

    // ── Shelf: the 2026-08-05 audit fixes ───────────────────────────────────────────────────

    #[test]
    fn shelf_go_leaves_a_working_back_button() {
        // Regression: `Go` called go(), which REPLACED the route stack with a single screen, so
        // Back (and the left-edge swipe) did nothing and the user was stranded on the pin.
        let mut a = unlocked();
        a.go(Screen::Library);
        a.open_shelf();
        a.tap(420, 582); // header Pin → slot 0
        a.tap(240, 200); // backdrop closes
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
        // Regression: tapping a bookmark's row BODY used to pin the current place into a
        // different slot instead of jumping to the bookmark; only a ~60px "GO" column worked.
        // And the same column on an EMPTY slot returned Go(i) — which did nothing but still
        // dismissed the sheet.
        let mut a = unlocked();
        a.go(Screen::Library);
        a.open_shelf();
        a.tap(200, 640 + 2 * 46 + 12); // body of empty slot 2
        assert!(a.pins[2].is_some());
        assert!(a.pins[0].is_none(), "it must pin where the finger was, not slot 0");
        assert!(a.toast.starts_with("Pinned to slot 3"), "{}", a.toast);
        assert!(a.shelf_is_open(), "pinning must not dismiss the sheet");
        a.tap(440, 640 + 2 * 46 + 12); // the × column forgets it
        assert!(a.pins[2].is_none());
        assert!(a.toast.starts_with("Slot 3 cleared"), "{}", a.toast);
    }

    #[test]
    fn shelf_restores_the_whole_place_not_just_the_screen() {
        let mut a = unlocked();
        let mut lib = Library::sample();
        lib.songs = (0..120)
            .map(|i| SongRow { title: format!("Track {i:03}"), object_id: i, ..Default::default() })
            .collect();
        a.set_library(lib);
        a.go(Screen::Library);
        a.lib_tab = Tab::Songs;
        a.lib_sort = 2;
        a.scroll_px(140);
        let scroll = a.lib_scroll_px;
        assert!(scroll > 0);
        a.open_shelf();
        a.tap(420, 582);
        a.tap(240, 200);
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
        // Regression: pins were session-scoped, so every reboot silently wiped the bookmarks.
        let mut a = unlocked();
        a.go(Screen::Library);
        a.lib_tab = Tab::Artists;
        a.lib_sort = 1;
        a.lib_scroll_px = 96;
        a.open_shelf();
        a.tap(420, 582);
        let encoded = a.shelf_pin_encode(0);
        assert!(!encoded.is_empty());
        let mut b = unlocked();
        b.shelf_pin_decode(0, &encoded);
        assert_eq!(b.pins[0], a.pins[0]);
        // Garbage in a hand-edited config clears the slot rather than panicking or half-loading.
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
        a.go(Screen::Library);
        a.scroll_px(200);
        let before = a.lib_scroll_px;
        a.open_shelf();
        a.scroll_px(300);
        a.fling(2000.0);
        a.tick();
        assert_eq!(a.lib_scroll_px, before, "the sheet must own the gesture");
    }

    #[test]
    fn a_modal_screen_is_not_a_place_you_can_pin() {
        let mut a = unlocked();
        a.go(Screen::UsbStorage);
        a.open_shelf();
        a.tap(420, 582);
        assert!(a.pins[0].is_none());
        assert_eq!(a.toast, "Nothing to pin here");
    }

    // ── UI scale ────────────────────────────────────────────────────────────────────────────

    /// Serialise every scale-sensitive test against the crate-wide lock in `text` — see
    /// `text::scale_guard` for why one shared lock and not a per-module one.
    fn lock_scale() -> crate::text::ScaleGuard {
        crate::text::scale_guard()
    }

    #[test]
    fn ui_scale_slider_scrubs_taps_and_steps() {
        let _scale = lock_scale();
        let mut a = unlocked();
        a.go(Screen::Settings);
        let row_y = crate::settings::LIST_TOP
            + crate::settings::row_top_px(crate::settings::ROW_UI_SCALE)
            + 10;
        // A tap on the track jumps straight to that stop (SeekBar idiom, not tap-to-cycle).
        assert_eq!(a.tap(460, row_y), vec![Action::UiScaleChanged]);
        assert_eq!(crate::text::scale_pct(), *crate::text::SCALE_STEPS.last().unwrap());
        // Dragging it scrubs live.
        assert!(a.scrub_begin(100, row_y));
        assert!(a.scrub_is_ui_scale());
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
        let base = crate::text::measure(&fonts, "Atlas Hands", &st);
        crate::text::set_scale_pct(140);
        let big = crate::text::measure(&fonts, "Atlas Hands", &st);
        assert!(big > base * 1.3, "measure() must follow the scale ({base} -> {big})");
        let mut c = Canvas::new();
        let pen = crate::text::draw(&mut c, &fonts, 0.0, 40.0, "Atlas Hands", &st);
        assert!((pen - big).abs() < 0.01, "draw() pen {pen} != measure() {big}");
    }

    #[test]
    fn library_tab_taps_land_on_the_labels_that_are_drawn() {
        let _scale = lock_scale();
        // Regression: the strip was LAID OUT from measured label widths but HIT-TESTED against
        // hardcoded thresholds (x<120/220/330). At the default size "ALBUMS" is drawn at
        // x≈94..154, so tapping its left half selected SONGS. Checked at three UI scales, since
        // the labels move with the scale and fixed thresholds could not have followed.
        let fonts = FontSet::load();
        for pct in [80u32, 100, 140] {
            crate::text::set_scale_pct(pct);
            let zones = library::tab_layout(&fonts);
            for (tab, x, w) in &zones {
                let mid = (x + w / 2.0) as i32;
                assert_eq!(tab_zone_at(&zones, mid), Some(*tab),
                           "scale {pct}%: tap at x={mid} picked the wrong tab");
            }
            // Every pixel of the strip belongs to some tab — no dead gaps between labels.
            for x in 0..crate::canvas::W as i32 {
                assert!(tab_zone_at(&zones, x).is_some(), "scale {pct}%: dead strip at x={x}");
            }
        }
    }

    // ── Brightness: level 0 is transient, never a trap ──────────────────────────────────────

    #[test]
    fn backlight_off_is_reachable_and_always_escapable() {
        let mut a = unlocked();
        a.go(Screen::Settings);
        a.settings_sel = crate::settings::ROW_BRIGHTNESS;
        // Cycle to the top, then one more lands on 0 = backlight off.
        for _ in 0..8 {
            if a.brightness() == 5 { break; }
            a.press(Button::Select);
        }
        assert_eq!(a.brightness(), 5);
        assert_eq!(a.press(Button::Select), vec![Action::BrightnessChanged(0)]);
        assert_eq!(a.brightness(), 0);
        // What gets PERSISTED is never 0 — a black panel must not survive a reboot.
        assert_eq!(a.brightness_restore(), 5);
        // Pressing again from 0 wraps to a VISIBLE level — the row can never stick on the dark one.
        a.press(Button::Select);
        assert_eq!(a.brightness(), 1);
        // And from 0, any input at all brings it back — to the last VISIBLE level, which the
        // press above moved to 1, not to a hardcoded default.
        assert_eq!(a.brightness_restore(), 1);
        a.brightness = 0;
        assert!(a.brightness_wake());
        assert_eq!(a.brightness(), 1);
        assert!(!a.brightness_wake(), "already awake → nothing to do, no needless backlight write");
    }

    #[test]
    fn a_corrupt_config_can_never_restore_a_black_panel() {
        let mut a = unlocked();
        a.set_brightness(0);
        assert_eq!(a.brightness(), 1, "0 in the config file means 'visible', not 'dark'");
        a.set_brightness(99);
        assert_eq!(a.brightness(), 5);
    }

    // ── Bluetooth: we say when we don't know ────────────────────────────────────────────────

    #[test]
    fn bluetooth_link_is_unknown_until_the_shell_reports() {
        let mut a = unlocked();
        assert!(a.bt_on);
        assert!(!a.bt_link_known(), "before the first poll, 'no device' would be a guess");
        assert_eq!(a.bt_connected(), None);
        assert!(a.set_bt_connected(Some("WH-1000XM4")));
        assert_eq!(a.bt_connected(), Some("WH-1000XM4"));
        assert!(a.bt_link_known());
        assert!(!a.set_bt_connected(Some("WH-1000XM4")), "no change → no repaint");
        // A reported disconnect is an observation, not a fallback to ignorance.
        assert!(a.set_bt_connected(None));
        assert_eq!(a.bt_connected(), None);
        assert!(a.bt_link_known());
    }
}
