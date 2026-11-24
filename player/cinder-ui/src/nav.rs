//! nav — the navigation state machine that turns hardware button presses into screen
//! transitions + playback actions. Keymap-AGNOSTIC: it speaks logical `Button`s; the
//! backend (cinder-ffi) maps raw evdev `/dev/input/hoge` key codes to
//! these (that raw map needs on-device `getevent` calibration — it isn't in any extracted
//! DTB). `App` owns *navigation* state (which screen, cursor positions, theme); live
//! now-playing data is passed into `render` by the shell. `press` returns `Action`s the
//! shell performs via cinder-audio (PlayerService) — the UI never touches audio directly.

use crate::bluetooth::Bt;
use crate::library::{self, Tab};
use crate::menu::MenuItem;
use crate::model::{AlbumRow, Library, SongRow};
use crate::now_playing::NowPlaying;
use crate::opt::Opt;
use crate::palette::Palette;
use crate::sound::Sound;
use crate::theme::{Accent, Tokens, CINDER};
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

/// The text size Big view starts from (`App::set_big_view`).
pub const BIG_TEXT_PCT: u32 = 130;

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
    /// Independent short-preview screen; entering or leaving it is only navigation.
    Zappin,
    Eq,
    Sound,
    /// Menu ▸ Soundscapes — procedural rain, a beach, a stream, wind, a fire, a night and three
    /// noises, over the music or on their own, each with its own level. See `soundscape.rs`.
    Soundscape,
    /// Sound ▸ Advanced — the rest of Sony's effect surface (Source Direct, Clear Phase, DSEE AI,
    /// DSEE HX Custom, Vinyl character, Tone Control). Its own screen because the Sound list is
    /// full and making it scroll would put a scroll offset into the one screen whose render and
    /// hit test have already drifted apart once. See `advanced.rs`.
    Advanced,
    /// Sound ▸ Advanced ▸ Tone Control — the three band gains. Its own screen because it is a
    /// slider field, not a row: the Advanced list is toggles and pills, and a three-column tap
    /// target does not fit inside a 64 px row. See `tone.rs`.
    Tone,
    /// Sound ▸ Advanced ▸ DAC EQ — five bands in the codec chip itself, applied by the setuid
    /// `cinder-voltable eq` helper. Same slider-field idiom as `Tone`. See `dac_eq.rs`.
    DacEq,
    /// Sound ▸ Advanced ▸ Volume curve — the owner's own headphone volume curve, applied by the
    /// setuid `cinder-voltable curve` helper. See `vol_curve.rs`.
    VolCurve,
    /// Sound ▸ Presets: saved sounds, and the A/B compare's tools. See `presets.rs`.
    Presets,
    /// Sound ▸ Noise cancelling, reachable only while Sony's NC earphones are in. See `nc.rs`.
    Nc,
    Bluetooth,
    /// Paired-device picker (connect / disconnect / forget). Pushed from Bluetooth ▸ "Pair new
    /// device"; before 2026-07-30 `pairing.rs` rendered but had no route at all.
    Pairing,
    /// Transmit codec + LDAC quality + Enhanced Mode, moved off the Bluetooth screen so the
    /// paired-device list can be its body.
    BtCodec,
    Settings,
    /// Settings ▸ Display — palette, accent, night, the volume readout, text size and the
    /// visualiser (design handoff 5k). The top five Settings rows moved here so the weekly
    /// preferences stop sharing a column with Restart and Reset. See `display.rs`.
    Display,
    /// Settings ▸ Display ▸ Palette — the palette picker (handoff 5j): every palette in the folder
    /// with its colours, the files that were refused and why, sortable by name or date added. See
    /// `palette_list.rs`.
    Palette,
    /// Settings ▸ Language — the installed languages, to pick one. See `language.rs`.
    Language,
    /// Display ▸ Style ▸ the Winamp or Cassette chip again: the skins (or tapes) on the drive, to
    /// pick one. See `skins.rs`.
    Skins,
    /// Settings ▸ Device — the hardware's vital signs: battery (level, voltage, health, charger
    /// state) plus die temperatures, CPU clock and governor, memory, storage and uptime. Its own
    /// screen because the Settings row could show exactly one fact, and because everything here is
    /// a world-readable file the app was already allowed to read and simply never did. Scrolls.
    /// See `device.rs`.
    Device,
    /// Settings ▸ Storage: each drive's space and what the music on it is made of.
    Storage,
    Fm,
    UsbDac,
    Receiver,
    Onboarding,
    /// Menu ▸ Help & controls: one scrolling list of what to do and how (design handoff 5i). The
    /// paged `Onboarding` intro runs once on the first start; this is the reference kept after it,
    /// and its last row replays the intro. See `help.rs`.
    Help,
    /// USB mass-storage mode: MODAL "connected to PC" screen while the storage volume is handed
    /// to the host. Only Back (or the shell detecting cable-unplug) leaves it.
    UsbStorage,
    /// Settings ▸ Visualiser — style, cover size, and the analyser's signal settings (level scale,
    /// dB range, response, curve, peak markers, the detector window and the frame rate), over a
    /// live preview of the real spectrum. Its own screen because it is nine controls you tune by
    /// watching, and because the Settings list is a list of unrelated preferences. See `vizset.rs`.
    VizSet,
    /// Settings ▸ Date & time — the clock editor. Its own screen rather than an in-row cycle: five
    /// fields is too much for a row, and setting a clock is a deliberate act that deserves an
    /// explicit Set rather than applying live under the finger.
    ClockSet,
    /// Settings ▸ Power ▸ Alarm (`alarm.rs`): wake and play at a time of day.
    Alarm,
    /// Settings ▸ Wi-Fi (`wifi.rs`). Reached only when the Wi-Fi install option is on the player.
    Wifi,
    /// Settings ▸ Wi-Fi ▸ Network: the networks in range; tap one to join, the joined one to forget.
    WifiNetworks,
    /// Settings ▸ Wi-Fi ▸ Internet radio (`netradio.rs`).
    NetRadio,
    /// Settings ▸ Wi-Fi ▸ Speakers: speakers on the network (UPnP/DLNA); tap one to play there.
    WifiSpeakers,
    /// Settings ▸ Wi-Fi ▸ Media servers: servers, then their folders (`media_server.rs`).
    MediaServers,
    /// Internet radio ▸ FIND: the stations a search found; tap one to add it and play it.
    RadioFind,
    /// Settings ▸ Wi-Fi ▸ Phone remote: the switch, the address and the code.
    WifiRemote,
    /// Settings ▸ Wi-Fi ▸ Options: Turn on at start, Set the clock, ListenBrainz, Check for updates.
    WifiOptions,
    /// Genre filter picker, pushed from the Library's filter strip. A picker, not a browse tab:
    /// 95 genres on the reference device is too many for a chip cycle and too few to deserve its
    /// own place in the tab strip.
    GenreFilter,
    /// Folder browse — the file tree as it is on the volume. Its own screen rather than a fifth
    /// Library tab: the tab strip is a flat set of four peers, and a folder view is a STACK you
    /// descend, so it needs Back to mean "up one level" rather than "leave the library".
    Folders,
    /// SensMe channels — browsing the library by Sony's own analysis of it. Two levels (the
    /// channel list, then one channel's tracks) on one screen, like `Folders`, with `sensme_channel`
    /// holding which is open. An OPT-IN component: the Menu row only exists when the installer left
    /// `/data/cinder/sensme_on`. See `sensme.rs`.
    SensMe,
    /// Audiobooks — the books under `LEARNING`, one row per folder, opened from the book button in
    /// the Library's header. Tapping a row plays the book from where the listener left it. See
    /// `audiobooks.rs`.
    Books,
    /// Track information — Sony's "Detailed Information". Pushed by tapping the title/artist/codec
    /// block on Now Playing, which is where the eye already is when the question comes up.
    TrackInfo,
    /// Lyrics for the playing song, pushed from the Lyrics row on Track information. Synced lyrics
    /// follow the song until the user scrolls. See `lyrics.rs`.
    Lyrics,
    /// Library search — the opt-in `search` component. Pushed from the Library header's button,
    /// with the keyboard opened on top of it. See `search.rs`.
    Search,
    /// Free text — naming or renaming a playlist. The device's only keyboard (`keyboard.rs`);
    /// there is no d-pad and no hardware keys to type with, so this is a touch grid.
    Keyboard,
    /// "Add to playlist": which of YOUR playlists does this track go into. Sony's own are not
    /// offered, because its database is not ours to write.
    PlaylistPick,
    /// "Add tracks": the library, tapped to add into the playlist that is open.
    TrackPick,
    /// Editing one of Cinder's own playlists (handoff 5b): move, remove, UNDO, DONE. Pushed from
    /// the playlist page's EDIT; nothing is written until DONE. See `playlist_edit.rs`.
    PlaylistEdit,
    /// Choosing a playlist's cover (`playlist_cover.rs`), from the editor's Cover row or the
    /// page's cover. Each choice is saved at once.
    PlaylistCover,
    /// The saved-view editor (handoff 5c): a smart playlist's name, rules and sort. Pushed from
    /// the Playlists tab's SMART half-row, or from a smart playlist's EDIT. See `view_edit.rs`.
    ViewEdit,
    /// Sentinel for the Menu row that opens the Shelf. The Shelf is a bottom-sheet OVERLAY (see
    /// `shelf_open`), never pushed onto the route stack — selecting this row calls `open_shelf()`.
    Shelf,
}

/// Why the keyboard is open. It carries the target with it, so the commit path is one match
/// rather than a set of flags that can disagree ("renaming" plus "creating" at once).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KbPurpose {
    /// Made from the Playlists tab: create it and open it.
    NewPlaylist,
    /// Made from "Add to playlist": create it, then put that track (object id) straight in — the
    /// whole reason the user is here, and losing it would mean naming a playlist and then having
    /// to find the track again.
    NewPlaylistWith(i64),
    /// Made from "Add to playlist" for a FOLDER: create it holding `App::take_pick_many()`.
    NewPlaylistWithPicked,
    /// Rename the playlist with this id.
    Rename(i64),
    /// Filter the "Add tracks" list. Unlike the others this commits no Action — the query is
    /// screen state, not something the shell has to carry out.
    TrackSearch,
    /// Library search. Like `TrackSearch`, screen state only: applied on every key, no Action.
    LibrarySearch,
    /// Save Up Next — the playing track and everything after it — as a new playlist.
    SaveQueue,
    /// Name the saved view being edited. Screen state only, like the searches: the text goes into
    /// the editor's draft, and nothing is written until the editor's SAVE.
    ViewName,
    /// Sound ▸ Presets: name the sound being saved (None), or rename preset `i`.
    PresetName(Option<usize>),
    /// Settings ▸ Wi-Fi: the password for `wifi_join_target`.
    WifiPassword,
    /// Internet radio ▸ FIND: a station name or genre to search radio-browser.info for.
    RadioFind,
    /// Settings ▸ Wi-Fi ▸ Media servers ▸ Search this server: a title, artist or album.
    MediaSearch,
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
/// where it lands in OUR list — the sequence it plays from is rebuilt at a track boundary (see
/// `Action::QueueChanged`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum QueueAt {
    /// Straight after the current track. Swipe LEFT on a row.
    Next,
    /// At the very end of Up Next, after everything already in it — so an album queued while
    /// another plays starts when that one finishes. Swipe RIGHT.
    Later,
}

/// How many played tracks Up Next's PREVIOUSLY PLAYED keeps. See [`App::history`] for the whole
/// retention policy and why each part of it is the way it is.
///
/// 100 rather than "everything": the list is persisted with the playback state, and that file is
/// rewritten on every settings change on a device whose `/contents` is vfat on flash. A hundred
/// object ids is ~700 bytes of it; an unbounded list on a 3,400-track library is not.
pub const HISTORY_MAX: usize = 100;

/// One complete sound SETUP: everything the Sound screen and the EQ own, as a value.
///
/// A and B are two of these, and the A/B control switches between them rather than bypassing the
/// chain. That is what the control is FOR — "does this EQ suit this album better than that one" is
/// the comparison people actually want, and on/off is a much less useful question that Sony
/// already answers with Source Direct.
///
/// Balance is INSIDE the setup, by explicit request (2026-08-17): it is part of how a pair of
/// headphones is dialled in, so it should travel with the rest of that dial-in.
/// VPT room names, in catalogue order — which is picker order, which is almost certainly enum
/// order. The index IS the value handed to Sony's `SetVptMode`; stock read back 1, i.e. Club.
/// See `snd_vpt_mode` for why these labels are provisional until an ear test settles them.
pub const VPT_MODES: [&str; 4] = ["Studio", "Club", "Concert Hall", "Matrix"];

/// DC Phase Linearizer filter types, in catalogue order — index IS the value handed to Sony's
/// `SetDcPhaseFilterType`. Stock read back 5, i.e. Type B High, which is the LAST member and is
/// decent evidence the list is complete. Same provisional-label caveat as [`VPT_MODES`].
pub const DC_PHASE_TYPES: [&str; 6] = [
    "Type A Low",
    "Type A Std",
    "Type A High",
    "Type B Low",
    "Type B Std",
    "Type B High",
];

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct SoundSetup {
    pub dsee: bool,
    pub vinyl: bool,
    pub vpt: bool,
    /// Index into [`VPT_MODES`]. Travels with the setup so A and B can hold different rooms.
    pub vpt_mode: usize,
    pub dc: bool,
    /// Index into [`DC_PHASE_TYPES`]. Travels with the setup, like the VPT room.
    pub dc_type: usize,
    pub norm: bool,
    pub clear: bool,
    /// L/R balance 0..=100, 50 = centre. Jack-only (see sound::Sound::bt_route), but still part of
    /// the setup — a wired listening setup is exactly where it matters.
    pub balance: usize,
    pub eq_preset: usize,
    pub eq_bands: [i8; 10],
    // ── Sound ▸ Advanced, in the setup since R5 (2026-09-30) ─────────────────────────────────
    // A setup is now a PROFILE: every DSP value Cinder can set, so an output can carry the whole
    // of its tuning (see `profile.rs`). Before R5 these were deliberately left out, on the grounds
    // that Advanced is "set once for a pair of headphones" — which is exactly what a per-output
    // profile is, so the reason for leaving them out became the reason for putting them in.
    /// Source Direct (bypasses the whole chain).
    pub src_direct: bool,
    /// Clear Phase (headphone).
    pub clear_phase: bool,
    /// DSEE AI. Present in the API, unverified on this hardware (see `advanced.rs`).
    pub dsee_ai: bool,
    /// DSEE HX Custom, and which of `advanced::DSEE_MODES`.
    pub dsee_custom: bool,
    pub dsee_mode: usize,
    /// Index into `advanced::VINYL_TYPES`.
    pub vinyl_type: usize,
    /// Tone Control in the path instead of the 10-band EQ, and its three bands (raw half-dB).
    pub tone: bool,
    pub tone_bands: [i8; crate::tone::BANDS],
    // ── optional members (2026-10-04) ────────────────────────────────────────────────────────
    // Mono, the linear headphone amp and the DAC EQ are ONE value for every output unless the
    // owner switches them into the profiles (Sound profiles ▸ ALSO PER PROFILE, each Off by
    // default; `App::profile_follow`). They are always carried here; while an item is not
    // followed, `App::swap_to_setup` keeps both profiles' copies equal to the live value.
    pub mono: bool,
    pub linear_amp: bool,
    pub dac_eq: [i8; crate::dac_eq::BANDS],
}

/// `App::profile_follow` bits: which of the optional members travel with a profile.
pub const FOLLOW_MONO: u8 = 1;
pub const FOLLOW_AMP: u8 = 1 << 1;
pub const FOLLOW_DAC_EQ: u8 = 1 << 2;
pub const FOLLOW_ALL: u8 = FOLLOW_MONO | FOLLOW_AMP | FOLLOW_DAC_EQ;

/// One line saying what a saved sound holds, for its Presets row: "Direct", or what is on.
pub fn describe_setup(s: &SoundSetup) -> String {
    if s.src_direct {
        return "Direct \u{b7} no effects".into();
    }
    let mut on: Vec<&str> = Vec::new();
    if s.tone {
        on.push(crate::i18n::text("Tone Control"));
    } else if s.eq_bands != [0; 10] {
        on.push("EQ");
    }
    if s.dac_eq != [0; crate::dac_eq::BANDS] {
        on.push(crate::i18n::text("DAC EQ"));
    }
    for (flag, name) in [
        (s.dsee, "DSEE HX"),
        (s.norm, crate::i18n::text("Normalizer")),
        (s.vinyl, crate::i18n::text("Vinyl")),
        (s.vpt, "VPT"),
        (s.dc, crate::i18n::text("DC Phase")),
    ] {
        if flag {
            on.push(name);
        }
    }
    if s.balance != crate::sound::BALANCE_CENTRE {
        on.push(crate::i18n::text("Balance"));
    }
    if on.is_empty() {
        "Flat \u{b7} no effects".into()
    } else {
        on.join(" \u{b7} ")
    }
}

impl Default for SoundSetup {
    fn default() -> Self {
        Self {
            dsee: false,
            vinyl: false,
            vpt: false,
            vpt_mode: 0,
            dc: false,
            dc_type: 0,
            norm: false,
            clear: false,
            balance: crate::sound::BALANCE_CENTRE,
            eq_preset: 3, // "A1"
            eq_bands: crate::data::EQ_PRESETS[3].1,
            src_direct: false,
            clear_phase: false,
            dsee_ai: false,
            dsee_custom: false,
            dsee_mode: 0,
            vinyl_type: 0,
            tone: false,
            tone_bands: [0; crate::tone::BANDS],
            mono: false,
            linear_amp: false,
            dac_eq: [0; crate::dac_eq::BANDS],
        }
    }
}

impl SoundSetup {
    /// The Advanced booleans in `App::adv_flags` bit order (bits 0..=4). Bit 5, the linear
    /// headphone amp, is hardware and NOT part of a profile, so it is never set here.
    pub fn adv_bits(&self) -> u8 {
        (self.src_direct as u8)
            | (self.clear_phase as u8) << 1
            | (self.dsee_ai as u8) << 2
            | (self.dsee_custom as u8) << 3
            | (self.tone as u8) << 4
    }
    /// Copy from `live` every optional member that `follow` does NOT name — the ones that are one
    /// value for every output.
    pub fn share_unfollowed(&mut self, live: &SoundSetup, follow: u8) {
        if follow & FOLLOW_MONO == 0 {
            self.mono = live.mono;
        }
        if follow & FOLLOW_AMP == 0 {
            self.linear_amp = live.linear_amp;
        }
        if follow & FOLLOW_DAC_EQ == 0 {
            self.dac_eq = live.dac_eq;
        }
    }
    /// The inverse of [`Self::adv_bits`]; bit 5 and above are ignored.
    pub fn set_adv_bits(&mut self, f: u8) {
        self.src_direct = f & 1 != 0;
        self.clear_phase = f & (1 << 1) != 0;
        self.dsee_ai = f & (1 << 2) != 0;
        self.dsee_custom = f & (1 << 3) != 0;
        self.tone = f & (1 << 4) != 0;
    }
}

/// Side effects the shell carries out (via cinder-audio / system services). The UI emits these
/// instead of acting on audio itself.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    PlayPause,
    /// Start/stop a short preview pass. The shell owns transport and timing; milliseconds are
    /// explicit so the 4s/15s choice cannot be confused with a UI row index.
    ZappinStart(u32),
    ZappinEnd,
    ZappinFull,
    ZappinLength(u32),
    ZappinNext,
    ZappinPrev,
    Next,
    Prev,
    NextAlbum,
    PrevAlbum,
    VolUp,
    VolDown,
    PlayIndex(i64), // play the library track with this DB object_id (the shell resolves it to a URI)
    /// `PlayIndex`, then seek this many ms into the track: a CUE track on an album page.
    PlayIndexAt(i64, u32),
    PlayPlaylist(i64), // play a whole playlist from the top (DB container object_id)
    /// `(playlist id, index)` — play a whole playlist STARTING AT one of its members.
    ///
    /// Tapping a track inside a playlist used to emit `PlayIndex`, and `PlayIndex` resolves an
    /// object id to its ALBUM context, because that is the only context an object id carries.
    /// So picking track 4 of a 60-track playlist played track 4's album and then stopped — on a
    /// playlist of singles, the one song. Reported 2026-08-23: "selecting a single song just
    /// plays that song not the rest of the playlist." The context has to travel WITH the tap,
    /// which is what this variant is for.
    PlayPlaylistAt(i64, u32),
    Shuffle(ShuffleScope), // the accent band on each Library tab — shuffle within that scope
    /// Shuffle ONE named artist: the shuffle button on an Artists row, or the band on that
    /// artist's page. The payload is a `lib.artists` INDEX (resolve it with
    /// `App::artist_name_at`) rather than the name, because `Action` is `Copy` and a `String`
    /// would take that away from every other variant.
    ShuffleArtist(usize),
    /// FM radio. The shell owns the tuner: power drives Open/Play + the ADC route, Tune is a
    /// single SetFrequency, Seek is a LIVE sweep (the chip's own auto-tune finds nothing on this
    /// hardware, so the shell steps and measures, reporting each step back so the dial moves),
    /// and Scan sweeps the whole band to fill the station list.
    FmPower(bool),
    FmTune(i32),
    FmSeek(i32),
    FmScan,
    /// Send the radio out over Bluetooth instead of the jack. The cable stays in as the aerial.
    FmBtOut(bool),
    /// Start (true) or stop (false) an FM recording.
    FmRecord(bool),
    /// Play a SensMe channel: `chan` is Sony's channel id (0..=12), or `sensme::ALL` for every
    /// analysed track; `from` is which member to start at; `shuffle` asks for it shuffled.
    ///
    /// One variant rather than the playlist surface's three, because a channel is not a database
    /// object: the shell resolves the membership out of the library it already holds, so "which
    /// channel, from where, in what order" is the whole message.
    PlaySensMe {
        chan: u8,
        from: u32,
        shuffle: bool,
    },
    /// Play an audiobook (an index into `Library::books`) from where the listener left it: its
    /// saved chapter at its saved position, or the first chapter for a book not started or
    /// finished. The shell owns the place, so the message is only which book.
    PlayBook(usize),
    /// Shuffle one playlist by DB id. Same channel as `PlayPlaylist`, but shuffled — the page's
    /// band needs it and `ShuffleScope::Playlist` picks a RANDOM playlist, which is a different
    /// thing entirely.
    ShufflePlaylist(i64),
    /// Create a playlist named `App::text_input()` (the keyboard's buffer). The name travels
    /// out-of-band because `Action` is `Copy` and a `String` in it would take that away from
    /// every other variant — the same reason `ShuffleArtist` carries an index, not a name.
    PlaylistCreate,
    /// Create it and add this track (object id) in one step.
    PlaylistCreateWith(i64),
    /// Create it holding every song of `App::take_pick_many()` (a folder and its subfolders, #56).
    PlaylistCreateWithPicked,
    /// `(playlist id)` — add every song of `App::take_pick_many()` to it (#56).
    PlaylistAddPicked(i64),
    /// Create it (named `App::text_input()`) holding Up Next: the playing track and everything
    /// after it, in play order (`App::queue_to_save`).
    PlaylistCreateFromQueue,
    /// Rename the playlist with this id to `App::text_input()`.
    PlaylistRename(i64),
    /// Delete it. Confirmed in the modal first — an m3u8 file is small but a curated list is not.
    PlaylistDelete(i64),
    /// Delete the music files of `App::take_delete()` from the player (Settings ▸ Delete music).
    /// Confirmed in the modal first; the shell rescans the library afterwards.
    DeleteMusic,
    /// `(playlist id, track object id)`.
    PlaylistAddTrack(i64, i64),
    /// `(playlist id, track object id)` — give the playlist the cover of that track's album, or
    /// clear it back to automatic with object id 0. A TRACK, not a path, because `Action` is `Copy`.
    PlaylistSetCover(i64, i64),
    /// `(playlist id, index into Library::cover_pictures)` — use a picture from the drive.
    PlaylistSetPicture(i64, u32),
    /// The playlist editor's DONE: write the edited order back to the `.m3u8`. The order itself
    /// travels out-of-band (`App::take_playlist_edit`), because `Action` is `Copy`.
    PlaylistSaveEdit(i64),
    /// `(track object id, stars 0..=5)` — the owner rated a track; 0 clears it. The UI has already
    /// updated its own copy (`Library::stats`); the shell writes it to the stats file.
    RateTrack(i64, u8),
    /// Play the list behind `App::take_play_list` SHUFFLED — a smart playlist's Shuffle band. The
    /// shuffled twin of `PlayListAt`, for a list that exists only as ids.
    ShuffleList,
    ThemeChanged(bool),
    Sleep,
    EnterUsbMsc,
    ExitUsbMsc, // leave USB mass-storage: the shell remounts the volume + restores the USB mode
    EqChanged([i8; 10]), // shell applies the band gains to the sound DSP
    BtToggle(bool), // shell turns the BT transmitter on/off
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
    /// Mono (accessibility) toggled. Its own action for the same reason `BalanceChanged` has one:
    /// nothing in Sony's DSP chain is involved, so re-applying the chain would be six EffectCtrlDmp
    /// round trips to change something none of them control. The shell reads `cinder_get_mono()`
    /// and applies it to every PCM path CINDER owns — see `analysis/RE_mono_audio.md` for why that
    /// is the whole of what it can reach.
    MonoChanged,
    /// Menu ▸ Soundscapes changed: on/off, the sound, or a level. The shell reads
    /// `ambient()` and rewrites `/tmp/cinder_ambient` (and starts or stops its own player). Sent
    /// per tap and per slider STEP, never per motion event; each one is a short tmpfs write.
    AmbientChanged,
    /// Sound ▸ Advanced ▸ DAC EQ changed. Its own action, not SoundChanged: nothing in Sony's DSP
    /// chain is involved. The shell reads `dac_eq()` (and Source Direct, which holds it flat) and
    /// runs `cinder-voltable eq`, which rebuilds the codec's tone table. Sent per tap and once at
    /// the end of a drag, never per motion event: each one is a helper run and a codec RAM reload.
    DacEqChanged,
    /// Sound ▸ Advanced ▸ Volume curve changed, or was switched on or off. The shell reads
    /// `vol_curve()` and runs `cinder-voltable curve` (or puts the installer's curve back), and
    /// writes `/contents/cinder_voltable.conf` so the launcher applies it at every start. Sent per
    /// tap and once at the end of a drag.
    VolCurveChanged,
    /// Display ▸ Style: the Winamp chip tapped while Winamp is already the style. The shell loads
    /// the next `.wsz` at the top of the drive, says which, and remembers it (`wsz=`).
    NextSkin,
    /// Sound ▸ Advanced ▸ Headphone EQ tapped: the shell plays the next profile in `cinder_eq`
    /// (off after the last), says which, remembers it (`hpeq=`) and calls `set_hp_eq`.
    NextHpEq,
    /// The Skins screen opened: the shell lists the skins (or tapes) with `set_skin_list`.
    ListSkins,
    /// Load the skin (or tape) at this index of `skin_names`.
    PickSkin(usize),
    /// The Winamp skin's ■: pause, and back to 0:00.
    Stop,
    /// BT Receiver switched (or left, which switches it off). The shell reads `rx_on()` and runs
    /// the whole enter / leave chain; idempotent, so a repeat is harmless.
    RxChanged,
    /// Receiver codec preference changed. The shell persists it and performs one guarded
    /// stop/set/start renegotiation only when the receiver is currently active.
    RxCodecChanged,
    /// Receiver page ▸ Use Sony's receiver, confirmed: restart into the stock player with its own
    /// Bluetooth receiver open. The shell writes stock's resume key, then boots to stock.
    BootToSonyReceiver,
    /// Balance only. Deliberately NOT SoundChanged: that action re-applies the whole DSP chain
    /// (six EffectCtrlDmp round trips) plus a settings write, and the balance slider emits on every
    /// motion event. This one lands on a single cached amixer call.
    BalanceChanged,
    /// Settings ▸ Date & time ▸ SET CLOCK. The shell reads `cinder_get_clock_epoch()` and runs the
    /// setuid `cinder-clock` helper, which writes BOTH the system clock and the RTC.
    ClockSet,
    /// Settings ▸ Wi-Fi switch: `cinder-wifi on` / `off`. The shell answers with a status.
    WifiPower(bool),
    /// Settings ▸ Wi-Fi ▸ SCAN: `cinder-wifi scan`; the shell answers with the list.
    WifiScan,
    /// A network chosen (and its password typed): the shell takes it with `take_wifi_join`, feeds
    /// it to `cinder-wifi join` on stdin, then runs `on`.
    WifiJoin,
    /// Settings ▸ Wi-Fi ▸ Network ▸ WPS: `cinder-wifi wps` (up to 2 min for the router's button),
    /// then `on`. The shell answers with a status.
    WifiWps,
    /// Settings ▸ Wi-Fi ▸ Set the clock switched on while connected: `cinder-wifi timesync` now
    /// (otherwise the shell runs it after each successful `on` while the setting is on).
    WifiTimeSync,
    /// Settings ▸ Wi-Fi ▸ the joined network ▸ Forget: `cinder-wifi forget`, then `off`.
    WifiForget,
    /// Settings ▸ Wi-Fi ▸ Send plays switched on while connected: send now
    /// (otherwise the shell sends after each connect and every 15 minutes while on).
    WifiListenBrainz,
    /// Settings ▸ Wi-Fi ▸ Check for updates switched on while connected: check now (otherwise once
    /// a day while connected).
    WifiUpdateCheck,
    /// Settings ▸ Wi-Fi ▸ Install Cinder v…, confirmed: the shell's Wi-Fi worker downloads and
    /// stages the release (`cinder_update_stage`), then restarts into Sony's updater.
    WifiUpdateInstall,
    /// Settings ▸ Wi-Fi ▸ Internet radio opened: the shell reads cinder_radio.txt again and hands
    /// it over with `set_radio_stations` (a file edited over USB shows up without a restart).
    RadioOpen,
    /// A station tapped: the shell takes it with `take_radio` and plays it.
    RadioPlay,
    /// The playing station tapped again: stop.
    RadioStop,
    /// Settings ▸ Wi-Fi ▸ Phone remote switched on (true) or off. cinder-ffi starts and stops the
    /// server itself; the shell only drains its button presses and uploads.
    Remote(bool),
    /// Settings ▸ Wi-Fi ▸ Phone remote ▸ Forget phones: every paired phone needs the code again.
    RemoteForget,
    /// Settings ▸ Wi-Fi ▸ AirPlay switched: the shell reads `cinder_wifi_airplay()` and starts or
    /// stops the receiver to match.
    WifiAirPlay,
    /// Settings ▸ Wi-Fi ▸ Speakers opened or SCAN tapped: cinder-ffi searches the network and hands
    /// the list back with `set_speakers`.
    SpeakersScan,
    /// Settings ▸ Wi-Fi ▸ Media servers opened, or its strip tapped: search for servers. The
    /// shell answers with `media_set_servers` (or `media_fail`).
    MediaSearch,
    /// A row of the Media servers page: `index` of the level `depth` deep (0 = the server list).
    /// A server or folder is browsed (`media_push`); a song plays, and the rest of its folder after
    /// it (`media_set_playing`).
    MediaOpen {
        depth: usize,
        index: usize,
    },
    /// Search this server ▸ Done: the shell searches the server open at level `depth` for
    /// `media_query()` and answers with `media_push` (a level of results, one deeper).
    MediaFind {
        depth: usize,
    },
    /// Speaker `i` of the list tapped: cinder-ffi plays Up Next there from the song playing.
    SpeakerPlay(usize),
    /// "Play here": the cast ends and the song carries on on the player.
    SpeakerStop,
    /// Internet radio ▸ FIND ▸ Done: cinder-ffi searches for `radio_query()` and hands the
    /// stations back with `set_radio_found`.
    RadioFind,
    /// Search result `i` tapped: cinder-ffi adds it to cinder_radio.txt and plays it.
    RadioAdd(usize),
    SoundBypass(bool), // A/B: true = bypass whole chain (B), false = re-enable (A)
    SleepTimer(u32),   // arm/cancel the sleep timer: minutes (0 = off); cinder-ffi counts down
    ShuffleToggle,     // Now Playing shuffle on/off (FFI rebuilds the active context)
    RepeatCycle,       // Now Playing repeat: off ↔ one (shell applies via SetOneTrackMode)
    /// The user queue changed. The shell applies it just before the current track ends: that gets
    /// the user's first choice ahead of the next context track without making the queue gesture
    /// pay PlayerService's pause/seek/play replacement cost.
    QueueChanged,
    Restart,        // confirmed in the modal: shell calls PowerMgrServiceClient::Reboot
    PowerOff,       // confirmed in the modal: shell calls SetStatus(PowerOff)
    BtCodecChanged, // device-wide BT transmit codec / LDAC quality changed; shell reads + applies
    /// "Use Enhanced Mode" toggled: shell reads `bt_enhanced()` and calls
    /// `BtTransmitterServiceClient::SetControlAbsoluteVolume` (slot 31).
    BtEnhancedChanged,
    /// THIS DEVICE ▸ Debug log switched: the shell reads `bt_debug_log()` and turns the HCI capture
    /// on, or off and copies it to the drive.
    BtDebugLog,
    UsbDacToggle(bool), // engage/disengage USB-DAC input routed to 3.5mm + BT/LDAC (the headline feature)
    /// SEEK within the current track, as permille (0..1000) of its duration. Emitted by the Now
    /// Playing progress rail. On device the shell drives the rail through cinder-ffi (which knows
    /// the duration in ms and suppresses position updates mid-drag); this variant is what the
    /// host sim sees, so both surfaces share ONE claim site — `App::scrub_begin`.
    Seek(u16),
    /// Play the CURRENT PLAY CONTEXT starting at index `n` — the sequence Up Next is displaying,
    /// in the order it is displaying it.
    ///
    /// `PlayIndex` cannot express this, for the same reason `PlayPlaylistAt` had to exist: an
    /// object id only knows its ALBUM, so tapping a row in Up Next re-derived the album context
    /// from the tapped track and played that instead. After "Shuffle all songs" the context is the
    /// whole library in a shuffled order, and jumping to a track four rows down therefore threw
    /// the shuffle away and started that track's album — reported 2026-09-02: "if i shuffle all
    /// songs and then try to jump to a specific song in the queue it deletes everything and starts
    /// playing that album."
    ///
    /// The context has to travel WITH the tap, and an INDEX is what travels: the shell already
    /// holds the resolved sequence, so this says "same list, different starting point" and nothing
    /// is rebuilt or reshuffled. That is also why it must not go through `apply_shuffle` — the
    /// order on screen is already the order to play.
    PlayContextAt(usize),
    /// Play the LIST THE USER TAPPED IN — the Songs tab in its current sort and filter, a folder,
    /// an artist's tracks — from row `n`. The ids travel in `App::play_list` (an `Action` is
    /// `Copy`); the shell takes them with `App::take_play_list`.
    ///
    /// These three used to emit `PlayIndex`, which resolves the tapped track's ALBUM: tap a song in
    /// a folder or in "Recently added" and you got its album, not the list on the screen. The rule
    /// everywhere else in the app is that the list you are looking at is the list you get.
    PlayListAt(usize),
    /// The UI text scale changed (Settings ▸ UI scale). Internal + persisted; no device call.
    UiScaleChanged,
    BrightnessChanged(u8), // panel brightness level 1..5; shell maps it onto the backlight node
    ScreenOffTimer(u32),   // idle screen-off timeout in seconds (0 = off); the shell counts idle
    BootToStock,           // arm a ONE-SHOT boot into Sony's player, then restart
    /// Settings ▸ Database: ask Sony's MediaStore to rescan the music tree. The shell does the
    /// SetConfig + Scan; the library reload is NOT part of this action — the existing db_signature
    /// watcher sees the store change and re-opens it on its own.
    RescanLibrary,
    ToggleLiked, // heart the currently playing track (cinder-ffi owns the set)
    /// Every preference just went back to its default (Settings ▸ Reset settings). The shell
    /// re-applies the whole chain from the UI's new state — EQ, sound effects, balance, high gain,
    /// backlight, volume — rather than being told about each one, because the point of a reset is
    /// that EVERYTHING moved and enumerating them here would be one more list to keep in step.
    SettingsReset,
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
/// The Menu's destinations, in order: `(screen, title, static second line)`. An EMPTY second line
/// means `App::menu_subtitles` fills it from live state (see the test that holds that contract).
///
/// The 2026-09 redesign (handoff 5a) took two rows out. Now Playing became the strip under the
/// header — "what is playing" is state, not a place — and Up Next went with it, since the queue is
/// one tap from Now Playing's toolbar. Folders and USB-DAC stay, although the mock drew them
/// elsewhere: until Library grows its Folders page, taking them off the Menu would leave them with
/// no way in. Equalizer left on 2026-10-04 (owner's call): Sound's Equalizer row (handoff 2a) is
/// the way in, and it names what is in the path, which the Menu row could not.
const MENU: [(Screen, &str, &str); 12] = [
    (Screen::Library, "Library", ""), // live: album/track counts
    // Folder browse — the file tree as it is on the volume. Not a fifth Library tab: the strip is
    // four flat peers and this is a stack you descend, where Back has to mean "up one level".
    (Screen::Folders, "Folders", ""), // live: folder count
    // SensMe channels. Only drawn when the component is installed (see `App::menu_visible`), which
    // is why this row is in the table rather than appended somewhere by hand: the table stays the
    // one place a Menu destination is declared.
    (Screen::SensMe, "SensMe", ""), // live: channels/analysed counts
    // The subtitle names the one thing that stops it working, because an empty jack and a broken
    // radio sound identical.
    (
        Screen::Fm,
        "FM radio",
        "Needs wired headphones as the aerial",
    ),
    (Screen::Sound, "Sound", ""), // live: which effects are on
    (Screen::Soundscape, "Soundscapes", ""), // live: the sound, or Off
    (Screen::Bluetooth, "Bluetooth", ""), // live: configured transmit codec
    // Only drawn when the Wi-Fi install option is there (`App::menu_visible`). Asked for in
    // discussion #53: three levels into Settings was too far for a switch used every day.
    (Screen::Wifi, "Wi-Fi", ""),     // live: off, or the network it is on
    (Screen::UsbDac, "USB-DAC", ""), // live: On/Off
    // Special Menu action intercepted by `activate_menu`; the target is only a placeholder so
    // this stays inside the existing Menu route and does not add a new exhaustive Screen case.
    (Screen::Zappin, "Preview", ""),
    // NOT "BT Receiver": it stays reachable where it belongs, from Bluetooth ▸ Receiver mode,
    // until it works.
    (Screen::Settings, "Settings", "Display · playback · system"),
    (
        Screen::Help,
        "Help & controls",
        "Buttons, swipes, the way back",
    ),
];

/// Each Menu destination's icon (Settings ▸ Menu icons, discussion #53).
fn menu_icon(s: Screen) -> crate::kit::Icon {
    use crate::icons;
    match s {
        Screen::Library => icons::library,
        Screen::Folders => icons::folder,
        Screen::SensMe => icons::note,
        Screen::Fm => icons::radio,
        Screen::Sound => icons::sound,
        Screen::Soundscape => icons::waves,
        Screen::Bluetooth => icons::bt,
        Screen::Wifi => icons::wifi,
        Screen::UsbDac => icons::usb,
        Screen::Zappin => icons::play,
        Screen::Settings => icons::settings,
        _ => icons::help,
    }
}

/// `App::home_screen` values, in `menu::HOMES` order.
pub const HOME_LIBRARY: u8 = 0;
pub const HOME_NOW_PLAYING: u8 = 1;
pub const HOME_MENU: u8 = 2;
pub const HOME_LAST: u8 = 3;
/// Their settings-file words, in the same order.
const HOME_WORDS: [&str; 4] = ["library", "now_playing", "menu", "last"];

/// Nominal frame period at 60 fps, in ms. The HUD/fling constants are expressed in these frames;
/// `tick_dt` converts real elapsed time into them so the durations hold at any actual frame rate.
const FRAME_MS: u32 = 17;

/// Screen-off (idle) timeout presets, in seconds. 0 = off and is first, so the cycle starts from
/// "no idle blank" and the feature is strictly opt-in.
pub const SCREEN_OFF_PRESETS: [u32; 5] = [0, 15, 30, 60, 120];

/// Auto power-off, in MINUTES. Sony's own (sid_4118) offers a similar ladder. 0 = off, and it is
/// the default: a device that switches itself off is behaviour the owner has to ask for, not
/// something to spring on them.
pub const AUTO_OFF_PRESETS: [u32; 4] = [0, 15, 30, 60];

/// Label for an auto power-off preset.
pub fn auto_off_label(mins: u32) -> String {
    if mins == 0 {
        crate::i18n::text("OFF").to_string()
    } else {
        crate::ui_format!("{mins} MIN", mins = mins)
    }
}

/// Label for a screen-off preset, matching the row's other mono values.
pub fn screen_off_label(secs: u32) -> String {
    match secs {
        0 => String::from(crate::i18n::text("OFF")),
        s if s < 60 => crate::ui_format!("{s} SEC", s = s),
        s if s % 60 == 0 => crate::ui_format!("{} MIN", s / 60),
        s => format!("{}:{:02}", s / 60, s % 60),
    }
}

/// The Menu's live row subtitles (see `App::menu_subtitles`). One field per Menu row whose caption
/// describes current state rather than being fixed text.
pub(crate) struct MenuSubtitles {
    pub library: String,
    pub folders: String,
    pub sensme: String,
    pub sound: String,
    pub bluetooth: String,
    pub usb_dac: String,
    pub soundscape: String,
    pub wifi: String,
}

#[derive(Clone, Debug)]
struct ToastTemplate {
    key: &'static str,
    args: Vec<(String, String)>,
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
    // ── STABLE IDENTITIES ─────────────────────────────────────────────────────────────────
    // The fields above are INDICES into lists the library rebuilds at every boot, and a pin
    // outlives that rebuild. Add one track and every index after it shifts, so a pin saved on
    // "anymore (Deluxe)" opened "burn" instead — reported 2026-08-19. The index is kept only as a
    // fallback for a pin written before this existed, or one whose target has since been deleted.
    /// `AlbumRow::album_id` of `album_view`, or -1.
    album_id: i64,
    /// `AlbumRow::album_id` of `album_expanded`, or -1.
    expanded_album_id: i64,
    /// Name of `artist_view` (artists are keyed by name, not id), or empty.
    artist_name: String,
    /// `PlaylistRow::id` of `playlist_view`, or -1.
    playlist_id: i64,
}

/// The playlist editor's working state (`playlist_edit.rs`): the playlist's rows as they were when
/// the editor opened, the order being built, and what each change replaced.
struct PlEdit {
    id: i64,
    name: String,
    /// The members when the editor opened. A snapshot, so a library refresh underneath an open
    /// editor cannot change what an index means.
    rows: Vec<SongRow>,
    /// The working order, as indices into `rows`. A removed row is simply absent.
    order: Vec<usize>,
    /// The order before each change, newest last — what UNDO pops.
    undo: Vec<Vec<usize>>,
    scroll_px: i32,
    /// `glued[i]`: `rows[i]` is glued to whatever row is above it in `order` (plan 1c). Tapped on
    /// and off on the row; it travels with the row when the row moves. `glued0` is as opened.
    glued: Vec<bool>,
    glued0: Vec<bool>,
}

impl PlEdit {
    /// Does the working order differ from the playlist as it was opened?
    fn changed(&self) -> bool {
        self.order.len() != self.rows.len()
            || self.order.iter().enumerate().any(|(i, &o)| i != o)
            || self.glue_in_order() != self.glued0
    }

    /// Each row's glue in the working order; the top row has nothing above it to be glued to.
    fn glue_in_order(&self) -> Vec<bool> {
        self.order
            .iter()
            .enumerate()
            .map(|(slot, &i)| slot > 0 && self.glued.get(i).copied().unwrap_or(false))
            .collect()
    }

    /// Move the row at `from` to `to` (remove, then insert — the order the drag previewed).
    fn move_row(&mut self, from: usize, to: usize) {
        if from == to || from >= self.order.len() || to >= self.order.len() {
            return;
        }
        self.undo.push(self.order.clone());
        let it = self.order.remove(from);
        self.order.insert(to, it);
    }

    fn remove_row(&mut self, at: usize) {
        if at < self.order.len() {
            self.undo.push(self.order.clone());
            self.order.remove(at);
        }
    }
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
            | Screen::Books
            | Screen::UpNext
            | Screen::Eq
            | Screen::Sound
            | Screen::Bluetooth
            | Screen::Settings
            | Screen::Fm
            | Screen::UsbDac
            | Screen::Receiver
            | Screen::NetRadio
    )
}

/// Serialise a place as a `|`-separated record — a Shelf pin, or the "Last screen" home. `|` and
/// newlines are stripped from the labels so a track title can't corrupt the config.
fn encode_pin(p: &ShelfPin) -> String {
    let clean = |s: &str| s.replace(['|', '\n'], " ");
    format!(
        "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
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
        p.album_id,
        p.expanded_album_id,
        clean(&p.artist_name),
        p.playlist_id,
    )
}

/// Read an `encode_pin` record. A malformed or short record is no place at all — a hand-edited
/// config must never keep the player from booting.
fn decode_pin(s: &str) -> Option<ShelfPin> {
    let f: Vec<&str> = s.split('|').collect();
    if f.len() < 12 {
        return None;
    }
    // The SAME whitelist the pin gesture enforces. `shelf_tap` refuses to pin a screen that is
    // not a "place", but this path did not re-check it, and the two are not equivalent:
    // `screen_token`/`screen_from_token` cover screens `pinnable` excludes (Tone, BtCodec), the
    // token table is an ON-DISK FORMAT that outlives any one build, and the settings file is
    // plain text a user can edit. A record naming one of those would restore into a mode nobody
    // asked for — exactly what `pinnable` exists to prevent. Enforced at both ends now, so the
    // rule cannot be true on one side and not the other.
    let screen = screen_from_token(f[0]).filter(|s| pinnable(*s))?;
    let num = |s: &str| s.trim().parse::<i64>().unwrap_or(0);
    let exp = num(f[4]);
    Some(ShelfPin {
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
        // Records written before identities existed stop at field 11. They decode to -1/""
        // and fall back to their indices — the old behaviour, for the one boot it takes to
        // re-save them.
        album_id: f.get(12).map(|v| num(v)).unwrap_or(-1),
        expanded_album_id: f.get(13).map(|v| num(v)).unwrap_or(-1),
        artist_name: f.get(14).map(|v| v.to_string()).unwrap_or_default(),
        playlist_id: f.get(15).map(|v| num(v)).unwrap_or(-1),
    })
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
        Screen::Books => "books",
        Screen::UpNext => "queue",
        Screen::Eq => "eq",
        Screen::Tone => "tone",
        Screen::DacEq => "daceq",
        Screen::VolCurve => "volcurve",
        Screen::Sound => "sound",
        Screen::Soundscape => "soundscape",
        Screen::Bluetooth => "bt",
        Screen::BtCodec => "btcodec",
        Screen::Settings => "settings",
        Screen::Fm => "fm",
        Screen::UsbDac => "usbdac",
        Screen::Receiver => "rx",
        Screen::NetRadio => "radio",
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
        "books" => Screen::Books,
        "queue" => Screen::UpNext,
        "eq" => Screen::Eq,
        "sound" => Screen::Sound,
        "soundscape" => Screen::Soundscape,
        "bt" => Screen::Bluetooth,
        "btcodec" => Screen::BtCodec,
        "settings" => Screen::Settings,
        "fm" => Screen::Fm,
        "usbdac" => Screen::UsbDac,
        "rx" => Screen::Receiver,
        "radio" => Screen::NetRadio,
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

/// Which slot of the library's tab strip is at screen-x `x`, using the strip as it was last drawn?
/// `Some(None)` is the FOLDERS link. Splits the gaps between labels down the middle, so no pixel
/// of the strip is dead. `None` only when nothing has been rendered yet, which a real tap can't
/// precede.
fn tab_zone_at(zones: &[(Option<Tab>, f32, f32)], x: i32) -> Option<Option<Tab>> {
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

/// What the user's finger is currently dragging along a horizontal control.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Scrub {
    None,
    /// Now Playing progress rail → seek.
    Progress,
    /// Settings ▸ UI scale slider.
    UiScale,
    /// Equalizer ▸ one band column, and Advanced ▸ Tone Control ▸ one band column. VERTICAL, unlike
    /// every scrub above it — which is the whole reason `scrub_move` carries y as well as x. Before
    /// these two existed the band fields were TAP-ONLY: one tap, one step, ten taps to cross a
    /// band and a hundred to shape a curve.
    ///
    /// The band is captured at finger-DOWN and never re-picked. Letting the column change mid-drag
    /// would mean a diagonal sweep silently rewrites bands you were only passing over, which is
    /// unrecoverable without an undo the screen does not have.
    EqBand(usize),
    ToneBand(usize),
    /// Sound ▸ Advanced ▸ DAC EQ ▸ one band column. PREVIEW ONLY while the finger moves: applying
    /// it runs a setuid helper and reloads codec RAM, so it is sent once, at release.
    DacEqBand(usize),
    /// Sound ▸ Advanced ▸ Volume curve ▸ one point. Applied once, at release, like the DAC EQ.
    VolCurvePoint(usize),
    /// Sound ▸ L/R balance slider. Applies LIVE on every motion event rather than at release: the
    /// point of dragging a balance control is hearing the image move under your finger. The cost is
    /// bounded because the shell's `apply_balance` caches the last raw pair and skips the mixer
    /// write when it hasn't changed — a full sweep is 100 UI steps but only ~24 distinct
    /// attenuation values per side.
    Balance,
    /// The Winamp skin's volume and balance sliders (`winamp.rs`). Live, like the balance.
    WaVolume,
    WaBalance,
    /// The Winamp skin's scrolling title, dragged by hand: where the finger went down (panel and
    /// skin x), the shift then, and whether it has moved — a press that never moves is a tap.
    WaTitle {
        x0: i32,
        y0: i32,
        lx0: i32,
        shift0: i32,
        moved: bool,
    },
    /// Soundscapes ▸ one of the two level sliders (0 = on its own, 1 = with music). Live, like the
    /// balance: you set a background level by hearing it against the foreground.
    AmbientLevel(usize),
}

pub struct App {
    stack: Vec<Screen>,
    language: crate::i18n::Language,
    /// What the installer put on this player, in `Language::ALL` order; never empty.
    languages: Vec<crate::i18n::Language>,
    pub night: bool,
    /// The chosen accent colour. Purely a render input — no shell action, no Sony service, so
    /// changing it costs one repaint and a settings write.
    pub accent: Accent,
    /// The active palette's colour tokens — what `render` builds the theme from. Copied out of
    /// `palettes` (or `theme::CINDER`) whenever the choice resolves, so a frame looks nothing up.
    palette: Tokens,
    /// Palettes read from the player's `cinder_palettes/` folder, in cycle order. The built-in
    /// Cinder palette is not in here: it is always present, always first, and cannot fail to load.
    palettes: Vec<Palette>,
    /// Index into `palettes`, or None for the built-in.
    palette_active: Option<usize>,
    /// The persisted choice, by id. Kept apart from `palette_active` because the folder can be
    /// unreadable when the settings load — /contents arrives late at boot — and a choice that
    /// resolved to nothing must still be written back as the user's choice, not quietly replaced
    /// by Cinder the next time anything else is saved.
    palette_wanted: String,
    /// Why files in the folder were not loaded, one line each. The shell logs them; the Palette
    /// row says how many, once.
    palette_skipped: Vec<String>,
    /// Palette picker sort: index into `palette_list::SORTS` (0 name, 1 date added).
    palette_sort: usize,
    palette_scroll_px: i32,
    language_scroll_px: i32,
    /// The Skins screen: the names on the drive, the one in use, and the scroll.
    skin_names: Vec<String>,
    /// The headphone EQ profile playing, by name; empty when off (the shell's `set_hp_eq`).
    hp_eq: String,
    skin_cur: String,
    skins_scroll_px: i32,
    /// Help & controls' scroll offset; reset each time it is opened from the Menu.
    help_scroll_px: i32,
    /// Settings was opened since the folder was last read. The shell rescans on its next frame, so
    /// a palette copied over USB shows up without a reboot.
    palettes_stale: bool,
    pub locked: bool,
    pub playing: bool,
    menu_idx: usize,
    lib_tab: Tab,
    lib_idx: usize,
    /// Library list scroll in PIXELS (live drag + fling; rows render at a sub-row offset).
    lib_scroll_px: i32,
    /// How far the Library's shuffle band has slid up under the tab strip, in px. Moved only by a
    /// real scroll gesture (drag or fling, via `scroll_px`), so it tracks the finger: down hides,
    /// up brings it back. Read through `lib_band()`, never directly — see `library::band_offset`.
    lib_band_hide: i32,
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
    /// Whether the open artist page shows its full SONGS list. Closed each time a page opens.
    artist_songs_open: bool,
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
    row_drag: Option<crate::up_next::RowDrag>,
    /// THE PLAY SEQUENCE — everything Up Next shows below PREVIOUSLY PLAYED, in the order it will
    /// play — and where inside it the current track sits.
    ///
    /// ONE list since 2026-09-23. From 2026-08-11 it was split in two: this "context" (the album,
    /// playlist or shuffle scope the user started) and a separate user queue whose picks always
    /// played before the context resumed. That made the one thing people most want from a queue
    /// impossible — "play this album, then that one" put the second album in FRONT of the rest of
    /// the first. Now a track or album is added straight after the playing track (Play next) or at
    /// the very end (Add to queue), and the list plays top to bottom.
    context: Vec<SongRow>,
    context_idx: usize,
    /// The context's object_ids in the order they were in BEFORE the first shuffle permuted them,
    /// so turning shuffle off can put them back. `None` once there is nothing to undo.
    ///
    /// Ids rather than a second `Vec<SongRow>`: the context can be the whole library after a
    /// "Shuffle all songs", and cloning 3600 rows would double several thousand heap strings to
    /// remember an ORDER. 8 bytes a track answers the same question for ~29 KB.
    pre_shuffle: Option<Vec<i64>>,
    /// The generator for `queue_shuffle` and Random album. Starts from a constant so tests replay;
    /// the shell reseeds it from the clock (`seed_shuffle`).
    rng: crate::shuffle::Rng,
    /// Set by the "keep Up Next" answer: `set_play_context` keeps what was still to come and plays
    /// it AFTER the new sequence, rather than dropping it. Consumed on use.
    queue_keep: bool,
    /// The user has ADDED to the list by hand (Play next / Add to queue) since it was started.
    /// That is what makes replacing it worth a question — an album the user merely started is not
    /// work they would lose. Reordering and removing do not set it: they edit what is there rather
    /// than build something new.
    hand_added: bool,
    /// WHICH songs were added by hand, so the question is asked only while one of them is still to
    /// come. The flag alone stayed set after the added song had played, so starting anything new
    /// still asked about an Up Next that was only the album's own rest (owner, 11-23). Empty with
    /// `hand_added` set = not known (a list restored from an old file): any upcoming row counts.
    added_ids: Vec<i64>,
    /// Whether the transport's shuffle is on, as last drawn. The shell owns the flag; the UI only
    /// needs it so the MIX chip can turn shuffle ON rather than scramble the list behind an icon
    /// that still says "in order".
    shuffle_on: bool,
    /// Scrollbar drag: the scroll offset and the finger y the grab started at, so travel is applied
    /// against a fixed anchor rather than accumulated per event.
    sbar: Option<(i32, i32)>,
    /// Fling (momentum) velocity in px/s for the current scrollable list; decays each tick.
    fling_v: f32,
    /// MONO — sum left and right so both ears get the whole mix. The accessibility setting.
    ///
    /// On `App` rather than in `SoundSetup`, so the A/B control does not carry it: A and B are two
    /// tunings to compare, and someone who needs mono needs it on both sides of any comparison.
    /// Persisted like the rest of the preferences.
    mono: bool,
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
    /// THIS DEVICE ▸ Debug log is recording. Not persisted — see `bluetooth::Bt::debug_log`.
    bt_debug_log: bool,
    /// Settings ▸ Bluetooth auto off. Persisted; the shell reads it every tick.
    bt_idle_off: bool,
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
    /// Which page of the paired list the Devices screen is showing. The list is windowed rather
    /// than truncated (see `pairing::MAX_PAIRED`), and this is the only state that needs. Wrapped
    /// on use, so it never has to be clamped when the list gets shorter.
    bt_paired_page: usize,
    bt_scanning: bool,
    /// A pairing prompt the radio is waiting on, pushed by the shell from the listener. While this is
    /// `Some` the Devices screen is modal — see `pairing::hit_prompt`.
    bt_prompt: Option<crate::pairing::Prompt>,
    /// BT Receiver: the user's switch, and what the shell last reported (phase 0 off, 1 waiting,
    /// 2 attached, 3 audio arriving; the rest raw from BtPlayerService).
    rx_on: bool,
    /// Receiver-only codec preference: Auto, LDAC, AAC, SBC. Independent of transmit codec.
    rx_codec_pref: u8,
    rx_phase: u8,
    rx_peer: String,
    rx_codec: u32,
    rx_freq: u32,
    rx_bitrate: u32,
    /// Row whose FORGET is armed (two-tap), and the row with a connect in flight. FORGET is cleared
    /// whenever the list is replaced; the connect is found again by name (see `bt_paired_add`).
    bt_forget_armed: Option<usize>,
    bt_connecting: Option<usize>,
    /// The device a connect is in flight for, which is what survives a list refresh, and how long
    /// its spinner has run. See `bt_connect_row`.
    bt_connecting_name: Option<String>,
    bt_connecting_ms: u32,
    /// Spinner phase in SECONDS for the "connecting"/"scanning" indicators. Advanced by tick_dt
    /// from real elapsed time (never a frame count — this project has already been bitten once by
    /// assuming 60 fps when the device renders at ~32), and only while something is actually in
    /// flight, so an idle screen stays completely static and repaints nothing.
    busy_phase: f32,
    /// Milliseconds left of the optimistic play/pause hold. See `set_playing_optimistic`.
    playing_hold_ms: u32,
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
    /// What this boot's Bluetooth stack offers beyond Sony's four (`bluetooth::codecs`), pushed by
    /// the shell from the `bt-aac` component: AAC in aptX HD's place, SBC sent as SBC XQ.
    bt_aac: bool,
    bt_sbc_xq: bool,
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
    /// libcinder_mono.so is loaded in Sony's SoundServiceFw, so mono reaches the jack and Bluetooth
    /// too (the shell reports it from the library's own marker file).
    mono_shim: bool,
    /// Menu ▸ Soundscapes. `ambient_sound` is kept while switched off, so the switch brings it back.
    ambient_on: bool,
    ambient_sound: u8,
    /// The two levels, in percent: with nothing else playing, and over the music.
    ambient_alone: u8,
    ambient_music: u8,
    /// Where the shell says the soundscape is going (the page's strip).
    ambient_route: crate::soundscape::Route,
    ambient_sel: usize,
    /// The host's live stream format, as the engine last reported it: (rate Hz, bit depth,
    /// channels). `None` means nothing is streaming, or nobody has told us yet.
    ///
    /// The USB-DAC panel used to print `INPUT : PCM 24BIT / 96.0 KHZ` as a hardcoded literal, which
    /// was wrong on the one occasion it was ever measured (the real stream was 32-bit at 44.1 kHz).
    /// Showing a fabricated hardware fact is worse than showing none, so the line stayed generic
    /// until the format could cross the FFI boundary. This is that.
    usb_dac_fmt: Option<(u32, u32, u32)>,
    /// What an AirPlay sender says is playing (`set_airplay_meta`).
    airplay_meta: crate::live::AirPlayMeta,
    /// llusbdac: installed (the shell found the module), asked for, and loaded (it then stays until
    /// the restart, so the switch locks on).
    usb_dac_ll_avail: bool,
    usb_dac_ll: bool,
    /// llusbdac's own numbers (`/proc/llusbdac`), while it is loaded.
    usb_dac_ll_stats: Option<crate::usbdac::LlStats>,
    /// The raw `BtSoundCodec` word A2DP actually negotiated, or 0 for "not known". Everything else
    /// on screen is the user's *preference*; A2DP negotiates, so a sink that cannot do LDAC lands on
    /// SBC while the preference still says LDAC. The enumerators are not mapped yet, so this is
    /// stored raw and rendered as a neutral label — see `negotiated_codec_name`.
    bt_codec_negotiated: u32,
    // ── Bluetooth fine volume (the vernier) ─────────────────────────────────────────────────────
    // AVRCP gives one step of 4 units in 127 — about 2 dB, and 32 usable steps for the whole range.
    // There is no finer command: absolute volume (`SetCurrentVolume`) is inert on this firmware,
    // measured on two sinks including Sony's own WH-1000XM4, where three absolute writes produced
    // no notification and no movement while the relative steps either side of them reported
    // normally. So a finer step has to come from the SOURCE, and the only source-side gain any
    // Sony library exposes is the 10-band EQ, in half-dB units (measured: a flat -10 dB EQ moves
    // the analyzer's reading by -9.5 dB, a flat -5 dB by -5.8 dB).
    //
    // `bt_fine` is the span in dB that one AVRCP step is subdivided across (0 = off). `bt_trim` is
    // the current subdivision, in half-dB units at or below zero — attenuation only, because
    // boosting toward a sink's own ceiling is how you clip an encoder.
    /// Volume limit: the user's on/off for the safe-listening cap. The CAP ITSELF is not stored
    /// here — the shell reads Sony's AVLS threshold live, because it is per output device
    /// (`adapt=1`), so a number cached in the UI would be wrong the moment you changed output.
    volume_limit: bool,
    /// Settings ▸ Ignore "The" in artists. Mirrored into `lib.ignore_the`, which every ordering
    /// function reads; the copy here is what survives a library reload.
    ignore_the: bool,
    /// Each Library tab's layout (`library::LibView`), by `Tab as usize`. Copied into the library
    /// like `ignore_the`, because the layout functions all read `&Library`.
    lib_views: [library::LibView; 4],
    bt_fine: u8,
    bt_trim: i8,
    /// Now Playing visualiser type (cinder_ui::viz index) + animation on/off (UI settings).
    viz_kind: u8,
    viz_size: u8,
    // ── the visualiser's SIGNAL settings (Settings ▸ Visualiser) ────────────────────────────────
    // Indices, not values: every one of these is a cycling row, and the row is the thing that owns
    // what the options are. `viz_cfg()` turns them into the struct the spectrum code takes.
    viz_scale: u8,    // vizcfg::Scale — dynamic auto-gain vs a fixed reference
    viz_range: u8,    // vizcfg::range_from_index — the display's dB window
    viz_response: u8, // vizcfg::response_from_index — the attack/decay pair
    viz_interp: u8,   // vizcfg::Interp — how twelve bands are stretched over the columns
    viz_peak_hold: bool,
    // The two the SHELL acts on rather than the renderer: they are arguments to Sony's analyzer
    // (SetCalcSamples / SetUpdateRate), so cinder-ffi only stores and reports them.
    viz_window: u8, // vizcfg::window_ms_from_index — the detector's averaging window
    viz_bands: u8,  // vizcfg::bands_from_index — how many columns the visualiser draws
    viz_rate: u8,   // vizcfg::rate_from_index — frames per second the service emits
    /// Cursor on the Visualiser screen. Not persisted — a cursor is where you were, not a setting.
    viz_sel: usize,
    /// Which Now Playing page is showing (see now_playing::NpPage). Persisted.
    np_page: u8,
    /// Settings screen cursor.
    settings_sel: usize,
    /// Settings ▸ Display cursor.
    display_sel: usize,
    /// Night on a schedule (Settings ▸ Display ▸ Night schedule): on, its times in minutes after
    /// midnight, and which side of them the last tick found, so it acts only at the two edges.
    night_sched: bool,
    night_from: u16,
    night_until: u16,
    night_in_window: Option<bool>,
    /// What the player opens on: an index into `menu::HOMES` (Library, Now Playing, Menu, Last
    /// screen). Chosen on the Menu (handoff 5a). Persisted as `home_screen=<word>`.
    home_screen: u8,
    /// For "Last screen": the place to reopen, read from the settings file. Applied once by
    /// `apply_home`, or when the first library arrives if that is later (the place may name an
    /// album, which only resolves against a loaded library).
    home_last: Option<ShelfPin>,
    /// The last of Library and Audiobooks you were on: what the Now Playing Library button opens.
    browse_books: bool,
    /// `apply_home` asked for `home_last` before a library was there to resolve it against.
    home_pending: bool,
    /// The volume readout: 0 = Full (the pill), 1 = Minimal (a 3 px bar under the status bar).
    /// Index into `display::VOLUME_HUDS`. Persisted as `volume_hud=full|minimal`.
    volume_hud: u8,
    /// Settings ▸ Display ▸ Style. Persisted as `style=cinder|nocturne|terminal`, and put on the
    /// theme every frame so every render sees it.
    style: crate::style::Style,
    /// The shell has a Winamp skin loaded, so the Winamp style is the landscape view
    /// (`winamp.rs`), with its counting-down time and folded window.
    winamp_skin: bool,
    /// The shell has a cassette tape loaded (`tape.rs`), so the Cassette style turns on its side.
    tape_loaded: bool,
    wa_remaining: bool,
    /// Characters the scrolling title has been dragged by (`winamp::View::title_shift`).
    wa_title_shift: i32,
    wa_playlist: bool,
    /// The queue index of the playlist's first row once it has been scrolled; None follows the
    /// playing track.
    wa_pl_first: Option<usize>,
    /// The skin's visualiser: 0 spectrum, 1 oscilloscope, 2 off.
    wa_vis: u8,
    /// The skin's analyzer look, `winamp::ANALYZER_STYLES`; the clutter bar's V steps it.
    wa_style: u8,
    /// Battery care (Sony "Itawari" charging, ~90% cap). Mirrors the device state; the shell reads
    /// the real value at boot via cinder_set_battery_care and applies toggles via the action.
    battery_care: bool,
    /// Live battery readouts, pushed by the shell (`cinder_set_battery_detail`) alongside the
    /// status-bar percentage. Held here rather than read on demand because the UI has no
    /// filesystem: everything it knows about the device arrives through the C ABI.
    batt_pct: u8,
    batt_status: String,
    batt_health: String,
    batt_mv: i32,
    batt_chg_state: i32,
    batt_chg_fault: i32,
    batt_raw: String,
    /// Die temperatures in millidegrees C: SoC, power IC, analog block.
    dev_temp_cpu: i32,
    dev_temp_pmic: i32,
    dev_temp_abb: i32,
    dev_cpu_khz: i32,
    dev_cpu_max_khz: i32,
    dev_cores_online: i32,
    dev_cores_total: i32,
    dev_governor: String,
    dev_mem_total_kb: i32,
    dev_mem_avail_kb: i32,
    dev_music_total_mb: i32,
    dev_music_free_mb: i32,
    dev_data_free_mb: i32,
    dev_uptime_s: i32,
    dev_kernel: String,
    dev_base_fw: String,
    device_scroll_px: i32,
    storage_scroll_px: i32,
    /// Display and the Menu only scroll in Big view; read through `display_scroll` and
    /// `menu_scroll`, which clamp them.
    display_scroll_px: i32,
    menu_scroll_px: i32,
    /// Bluetooth ▸ Sound quality, which also only scrolls in Big view; read through
    /// `bt_codec_scroll`.
    bt_codec_scroll_px: i32,
    /// A rescan has been asked for and the library has not come back yet. Purely a label state.
    ///
    /// It needs TWO ways out, not one. `set_library` clears it when a reloaded library arrives,
    /// which is the normal case — but a scan that finds nothing changed writes nothing, the
    /// db_signature watcher therefore never fires, and no library ever comes back. Clearing only on
    /// reload would leave "Rescanning…" on screen for the rest of the session precisely when the
    /// answer was "already up to date". cinder-ffi owns the second exit, a deadline (see
    /// `rescan_until_ms` there), because that is where the frame clock lives.
    rescanning: bool,
    /// Sound Settings effect toggles. Each maps to an EffectCtrlDmp boolean setter (the shell reads
    /// these via cinder_get_sound_flags after a SoundChanged action). VPT/DC-Phase are on/off here;
    /// their mode/type (Studio/Club, Standard/Low) is a device-gated enhancement (enum values TBD).
    snd_dsee: bool,
    snd_vinyl: bool,
    snd_vpt: bool,
    /// Which VPT room, 0..=3 — an index into [`VPT_MODES`], sent straight through to Sony's
    /// `SetVptMode`. Only meaningful while `snd_vpt` is on; the two are kept separate because the
    /// device has separate `SetVpt(bool)` and `SetVptMode(enum)` calls, and stock leaves a mode
    /// behind even with VPT off (it read back mode 1 = Club).
    ///
    /// LABELS ARE PROVISIONAL. Catalogue order (the UTF-16BE .qm files) is Studio/Club/Concert
    /// Hall/Matrix and is almost certainly enum order, but an echoed read-back does NOT bound an
    /// enum on this device — values 0,3,4,5 all sounded distinct in an ear test, which more than
    /// four labels can explain. The mechanism is right either way; if the ear test renames them it
    /// is a one-line change to VPT_MODES.
    snd_vpt_mode: usize,
    snd_dc: bool,
    /// Which DC Phase filter, 0..=5 — an index into [`DC_PHASE_TYPES`], sent straight to Sony's
    /// `SetDcPhaseFilterType`. Separate from `snd_dc` for the same reason the VPT room is separate
    /// from `snd_vpt`: the device has separate on/off and type calls.
    snd_dc_type: usize,

    // ── Sound ▸ Advanced ────────────────────────────────────────────────────────────────────
    // Sony controls Cinder never exposed. Their setters were verified present in
    // libEffectCtrlDmp.so's dynamic table before being wired; see analysis/RE_dsp_effects_surface.md.
    /// Bypasses the entire chain. Sony's own control, distinct from Cinder's A/B, which uses
    /// DisableSoundEffects.
    adv_source_direct: bool,
    adv_clear_phase: bool,
    adv_dsee_ai: bool,
    /// Sound ▸ Advanced ▸ Linear headphone amp. Off by default; the shell re-asserts it each second
    /// while on (main.cpp hp_amp_tick).
    adv_hp_linear: bool,
    /// DSEE HX Custom on/off, and which mode — same split as VPT, same reason.
    adv_dsee_custom: bool,
    adv_dsee_mode: usize,
    /// Vinylizer character, 0..=3. Always meaningful to SET even with the Vinyl Processor off:
    /// the device read back type=7, which is not a member of a four-value enum, because nothing
    /// had ever set it.
    adv_vinyl_type: usize,
    adv_tone: bool,
    /// Index into `advanced::CLEAR_BASS`, and whether this player's firmware can run it at all.
    // ponytail: one value for every output, not per sound profile. Join `SoundSetup` if asked.
    adv_clear_bass: usize,
    clear_bass_available: bool,
    /// The shell's report that this player runs Walkman One's sound library (no `vpt`,
    /// `clearphase` or `dseeai` in its chain).
    w1_chain: bool,
    /// FM radio state. Frequency in kHz — the unit `SetFrequency` takes, and it validates its own
    /// range, so an out-of-band value is rejected rather than stored.
    fm_khz: i32,
    fm_playing: bool,
    fm_stations: [i32; crate::fm::PRESETS],
    fm_n_stations: usize,
    fm_scanning: bool,
    fm_scan_pct: u8,
    /// Something in the headphone jack. The cable IS the aerial, so without it every frequency is
    /// noise and the screen says so rather than looking broken.
    fm_antenna: bool,
    fm_bt_out: bool,
    /// The FM recorder is installed (shell-reported); the screen shows REC only then.
    fm_rec_available: bool,
    /// Seconds recorded so far as the shell reports them, or -1 when not recording.
    fm_rec_secs: i32,
    /// Live RSSI straight off the Si4708 (STATUS_RSSI[7:0]), or -1 when the register path is down
    /// and there is no meter to draw. NOT Sony's GetSignalLevel, which is a constant 1.
    fm_signal: i32,
    /// The chip's registers are reachable, so the meter and seek are hardware rather than measured.
    fm_hw: bool,
    /// Chip ST bit — a real stereo lock. Rare on this aerial, which is why it is worth showing.
    fm_stereo: bool,
    /// Tone Control band gains, RAW half-decibels, ±20 = ±10 dB — measured on device, not assumed
    /// (`cinder-probe --tone`). Index order is Sony's own: 0 BASS, 1 MIDDLE, 2 TREBLE, confirmed
    /// by the sound service logging `eqtone,type=N` as each is written.
    tone_bands: [i8; crate::tone::BANDS],
    /// Focused band on the Tone Control screen.
    tone_sel: usize,
    /// DAC EQ band gains, RAW half-decibels, -24..=12 (-12..+6 dB) — the setuid helper's own
    /// range, which it enforces by REFUSING anything outside it. Band order is the helper's
    /// argument order: bass shelf, 400 Hz, 1.5 kHz, 4 kHz, treble shelf. See `dac_eq.rs`.
    dac_eq: [i8; crate::dac_eq::BANDS],
    /// Focused band on the DAC EQ screen.
    dac_eq_sel: usize,
    /// Sound ▸ Advanced ▸ Volume curve: Sony's step at volume 0, 20 … 120 (`vol_curve.rs`).
    vol_curve: [u8; crate::vol_curve::POINTS],
    /// The owner's curve is in use.
    vol_curve_on: bool,
    /// `cinder-voltable` is installed (the shell reports it).
    vol_curve_ok: bool,
    vol_curve_sel: usize,
    /// Focused row on the Advanced screen.
    adv_sel: usize,
    /// Sound ▸ Advanced scroll offset (px): twelve rows are taller than the glass.
    adv_scroll_px: i32,
    snd_norm: bool,
    snd_clear: bool,
    /// L/R balance position 0..=100, 50 = centre. Persisted alongside the other sound settings.
    snd_balance: usize,
    /// The setup NOT currently selected. The live one is the `snd_*` / `eq_*` fields above, so
    /// every existing reader keeps working unchanged; switching swaps this with those. Holding the
    /// inactive one rather than an index into an array of two is what keeps the refactor small.
    setup_other: SoundSetup,
    /// 0 = A is live, 1 = B is live.
    setup_idx: usize,
    /// Saved sounds (Sound ▸ Presets), at most `presets::MAX`. Persisted as `preset<N>_*` keys.
    presets: Vec<SoundSetup>,
    /// Their names, by index; empty for "Preset N". Persisted as `preset<N>_name`.
    preset_names: Vec<String>,
    /// How far Sound ▸ Presets is scrolled.
    presets_scroll_px: i32,
    /// The preset whose Delete was tapped once; a second tap deletes it.
    preset_armed: Option<usize>,
    /// Track info ▸ ALBUM SOUND: `(album key, preset 1-based)`. Persisted as `album_sound=`.
    album_sounds: Vec<(String, u8)>,
    /// The album the follow last looked at, so it acts once per album, not once per track.
    album_sound_key: Option<String>,
    /// The sound that was live before an album's preset took over; put back when it ends.
    album_sound_prev: Option<SoundSetup>,
    /// Which of mono / linear amp / DAC EQ travel with a sound through A/B and presets
    /// (`FOLLOW_*`). 0 = none, the default: one value for both sides.
    profile_follow: u8,
    /// The output the shell last reported, so a route change is seen once.
    live_output: crate::profile::Output,
    /// A route change moved the jack's EQ between the DAC and Sony's 10-band (`jack_eq_dac`), and
    /// the shell has not re-applied the chain yet.
    profile_apply: bool,
    /// On the headphone jack the curve lives in the DAC chip (Sound ▸ Advanced ▸ DAC EQ), which
    /// costs no processor time, and Sony's 10-band is held flat; Bluetooth still uses the 10-band.
    /// The default since 2026-10-18 (the owner's call); `jack_eq=10band` in the settings file
    /// puts the 10-band back on the jack.
    jack_eq_dac: bool,
    /// Sony NC earphones (a 5-pin plug) are in the jack — the shell's report, from the codec.
    nc_plug: bool,
    /// Sony's ambient-sound filter is on this player (Walkman One deleted it).
    nc_ambient_ok: bool,
    /// Index into `nc::MODES` and `nc::MODELS`. Persisted; applied only while `nc_plug`.
    nc_mode: usize,
    nc_model: usize,
    /// Sound screen scroll offset (px). The grouped list is taller than the glass.
    sound_scroll_px: i32,
    /// The firmware has ClearAudio+ (the shell says no after a write that did not take).
    clearaudio_ok: bool,
    /// Wall-clock epoch (UTC seconds), pushed by the shell about once a second. 0 = never reported,
    /// in which case the Date & time row falls back to the status-bar string rather than showing a
    /// confident 1970. NOT persisted: it is the system clock, not a preference.
    clock_epoch: i64,
    /// The clock editor's working copy. Seeded from `clock_epoch` when the screen opens, and left
    /// alone while it is open — otherwise the per-second push would drag the field you are editing
    /// out from under your finger.
    clock_fields: crate::clockset::Fields,
    clock_sel: usize,
    /// Settings ▸ Power ▸ Alarm: on, its time (minutes after midnight) and the selected row.
    alarm_on: bool,
    alarm_min: u16,
    alarm_sel: usize,
    /// Settings ▸ Wi-Fi: the last status and scan the shell reported, and whether a scan is out.
    wifi_status: crate::wifi::Status,
    wifi_networks: Vec<crate::wifi::Network>,
    wifi_scanning: bool,
    /// The network whose password the keyboard is asking for.
    wifi_join_target: Option<Vec<u8>>,
    /// A WPS join is waiting for the router's button; cleared by the next status.
    wifi_wps: bool,
    /// A join for the shell to carry out (SSID bytes, password; empty for an open network). Taken
    /// once by `take_wifi_join`, so the password does not stay in memory longer than it must.
    wifi_join: Option<(Vec<u8>, String)>,
    /// What the AirPlay receiver is doing, and the DLNA renderer (Wi-Fi ▸ Options ▸ Play to).
    airplay_note: String,
    play_to_note: String,
    /// Settings ▸ Wi-Fi ▸ Options ▸ Share the drive (`Opt::WifiDrive`): its address while on, and
    /// the code in that address (saved, so a computer's mapping lasts).
    drive_note: String,
    drive_code: String,
    /// What the shell last said about each (shown as the row's second line).
    wifi_lb_note: String,
    wifi_update_note: String,
    /// The newer release the last check found ("v0.4.1"); empty = none, no install strip.
    update_offer: String,
    /// What the install is doing, or why it stopped (the strip shows it in place of the offer).
    update_install_note: String,
    /// Install confirmed and not yet failed: the row ignores taps.
    update_running: bool,
    /// Internet radio: the stations from the drive, what the shell says, and the pick it takes.
    radio_stations: Vec<crate::netradio::Station>,
    /// How far Internet radio's station list is scrolled.
    radio_scroll_px: i32,
    radio_status: crate::netradio::Status,
    radio_pick: Option<usize>,
    /// Settings ▸ Wi-Fi ▸ Phone remote: serving now (the switch itself is `Opt::WifiRemote`, saved),
    /// its code and port, and how many phones are paired.
    remote_on: bool,
    remote_phones: usize,
    remote_code: String,
    remote_port: u16,
    /// The link went while the remote was on: cinder-ffi stops the server at the next frame.
    remote_dropped: bool,
    /// Settings ▸ Wi-Fi ▸ Speakers: (name, model) of each speaker found, a search running, the
    /// speaker a cast is playing on, and the page's strip.
    speakers: Vec<(String, String)>,
    speakers_scanning: bool,
    /// Settings ▸ Wi-Fi ▸ Media servers: the levels walked so far (the server list first), whether
    /// the shell is loading one, what went wrong last, and the song playing from a server.
    media_levels: Vec<crate::media_server::Level>,
    media_busy: bool,
    media_note: String,
    media_playing: String,
    media_query: String,
    /// Internet radio ▸ FIND: the words searched for, what came back, a search running, and why
    /// nothing came back.
    radio_query: String,
    radio_found: Vec<crate::netradio::Found>,
    radio_finding: bool,
    radio_find_note: String,
    cast_speaker: Option<String>,
    cast_line: String,
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
    /// The keyboard's buffer and modifiers. They live here because the keyboard is a ROUTE, not a
    /// modal call: the text has to survive every frame between one key and the next.
    kb_text: String,
    kb_shift: bool,
    kb_page: u8,
    kb_purpose: KbPurpose,
    /// The track "Add to playlist" is about (object id; 0 = nothing).
    pick_track: i64,
    /// …or the songs of a folder and everything below it, when the picker was opened by holding a
    /// folder row (#56). Empty = the single-track picker. With its name, for the picker's header.
    pick_many: Vec<i64>,
    pick_many_name: String,
    /// Folders ticked for "Add to playlist" on the current level (#56). Empty = not selecting.
    folder_sel: Vec<usize>,
    /// The playing playlist's glue (plan 1c): `(a, b)` = song `b` is glued after song `a`. Set by
    /// the shell when one of the user's playlists starts, cleared by any other play, and honoured
    /// by every shuffle wherever the pair is still side by side.
    glue_pairs: std::collections::HashSet<(i64, i64)>,
    pick_scroll_px: i32,
    track_pick_scroll_px: i32,
    /// The song order the track picker is showing (indices into `lib.songs`). Built once when the
    /// screen opens: sorting 3,945 rows per frame is exactly the kind of per-frame derived work
    /// the 2026-08-16 performance pass went and removed everywhere else.
    track_pick_order: Vec<usize>,
    cover_scroll_px: i32,
    /// The playlist editor's working state, while `Screen::PlaylistEdit` is open.
    pl_edit: Option<PlEdit>,
    /// The order the editor's DONE produced, and each member's glue, waiting for the shell.
    pl_edit_done: Option<(i64, Vec<i64>, Vec<bool>)>,
    /// The saved views (`views.rs`), in the order they were made. Each draws as a smart playlist
    /// at the top of the Library's Playlists tab. Persisted by the shell in `cinder_views.conf`.
    views: Vec<crate::views::SavedView>,
    /// The saved-view editor's draft, while `Screen::ViewEdit` is open: which view it is editing
    /// (None = a new one) and the working copy. Nothing reaches `views` until SAVE.
    view_edit: Option<(Option<usize>, crate::views::SavedView)>,
    /// What the shuffle button deals: songs, albums or artists (Settings ▸ Shuffle). Persisted as
    /// `shuffle_by=<word>`.
    shuffle_by: crate::shuffle::ShuffleBy,
    /// What the pending `Action::ShuffleList` deals — set with `play_list`, taken with it.
    list_shuffle_by: crate::shuffle::ShuffleBy,
    /// The song a QueueOnPlay prompt is about. Held only while that modal is up: the tap has
    /// already happened, but which action it becomes depends on the answer.
    /// The play action a queue-replace prompt is holding, replayed verbatim on Confirm.
    ///
    /// It stores the ACTION, not a bare object id: the prompt sits in the middle of every play
    /// funnel, and a funnel that forgets which context you started from is the same defect as
    /// having no context at all (see `Action::PlayPlaylistAt`).
    pending_play: Option<Action>,
    /// What comes after the playing song in the list it was tapped in again, held while the
    /// "This is the current song" menu is up (`start_play_action`). Queued by RestNext.
    pending_rest: Vec<crate::model::SongRow>,
    /// The ids behind a pending `Action::PlayListAt`, in play order. See that action.
    play_list: Vec<i64>,
    /// How many tracks are liked — drives the Library's "Liked songs" row. The set itself lives in
    /// cinder-ffi (it owns persistence); nav only needs the count to render.
    liked_count: usize,
    /// Settings is taller than the panel, so it scrolls like the library lists.
    settings_scroll_px: i32,
    /// The Settings page on screen: `None` for home, else a `settings::GROUPS` index.
    settings_page: Option<usize>,
    /// Screen-off (idle) timeout in SECONDS; 0 = off, which is the default — nothing changes unless
    /// the user opts in. `screen_off_idx` cycles the presets from the Settings row.
    screen_off_idx: usize,
    screen_off_s: u32,
    /// Panel brightness, 1..=5 (Settings row cycles it). Persisted.
    ///
    /// PARKED FEATURE — "backlight off". The row briefly had a 6th stop at 0, and it was removed on
    /// 2026-08-26 because it did not do what it was for. The intention was **backlight off with the
    /// LCD content still visible** — unlit but readable in ambient light, the way a transflective
    /// panel works. Writing 0 to this panel's backlight just gives a black screen, which is a
    /// different and much less useful thing.
    ///
    /// Everything the feature needs is still here and still correct, so bringing it back is a small
    /// job IF the panel turns out to support it: the shell keeps its `lvl == 0` fully-off branch,
    /// `brightness_restore` keeps 0 out of the settings file, and `brightness_wake_on_input()`
    /// restores the level on the Hold switch or Power. What is missing is the panel-side question
    /// nobody has answered — whether mtkfb/the LCM can drop the backlight while leaving the segment
    /// drive on. Until then the row does not offer a stop that lies about what it does.
    brightness: u8,
    /// The last VISIBLE brightness level. Level 0 (backlight off) is a transient state, not a
    /// setting: it is never persisted, and the shell restores this value on the next input. So
    /// there is no way — not even across a reboot — to end up on a black panel with no way back.
    brightness_restore: u8,
    sleep_min: u32,
    /// Sleep ▸ End of song: pause when the playing song ends ("stop after current"). Not
    /// persisted, like the timer: a reboot is not the song ending.
    stop_after: bool,
    /// First-run onboarding: which page is showing, and whether the intro has been completed (the
    /// latter is persisted, so it only appears once; the Menu can re-open it any time).
    onboarding_page: usize,
    onboarding_seen: bool,
    /// The browsable library. Defaults to the design sample; the shell replaces it with the
    /// real DB contents via `set_library` after `cinder_db_open`.
    lib: Library,
    /// Shelf bottom-sheet overlay: whether it's showing, and the three pin slots (jump-back places).
    shelf_open: bool,
    /// The opt-in switches (`opt.rs`): pull-down panel, Folders tab, Delete music, Menu icons, Now
    /// Playing's back arrow and track number, Hold locks the buttons, and the Wi-Fi options. All
    /// OFF by default.
    opts: crate::opt::Opts,
    /// What the open delete question would remove, as object ids. Set when it is ASKED, so the
    /// answer deletes exactly what the question was about even if the song changes meanwhile.
    delete_ids: Vec<i64>,
    /// What a swipe up / down in the middle of Now Playing opens (Settings, #51). Off by default.
    np_swipe_up: crate::settings::SwipeTo,
    np_swipe_down: crate::settings::SwipeTo,
    /// Now Playing's third toolbar button (Settings ▸ Now Playing button, #51). Bluetooth by default.
    np_button: crate::now_playing::NpButton,
    /// Where Now Playing was opened from, for Back on it (the left-edge swipe, the back arrow).
    /// Opening Now Playing resets the stack (`go`), so without this Back there had nowhere to go
    /// and did nothing (owner, 2026-11-03: "hard to activate on the now playing screen").
    np_from: Vec<Screen>,
    /// The pull-down panel is showing. Its own flag, not the Shelf's: the two never share state.
    quick_open: bool,
    pins: [Option<ShelfPin>; crate::shelf::SLOTS],
    /// PLAY HISTORY — what has actually been played, oldest first. Up Next's PREVIOUSLY PLAYED.
    ///
    /// It used to be `context[..context_idx]`, which is not a history but a way of spelling
    /// "earlier in this album": starting a different album erased everything before it, and a
    /// swipe-queued pick could never appear at all, because a pick is not in the context. This
    /// list is independent of both, so it survives a context change the way the thing it is named
    /// after does.
    ///
    /// RETENTION, decided 2026-09-15 (the user left the policy to this side):
    ///
    ///   * **Capped at [`HISTORY_MAX`]**, oldest dropped. Long enough to walk back through an
    ///     evening, short enough that it costs nothing to keep or to persist.
    ///   * **Consecutive repeats are collapsed.** Repeat-one is a supported mode on this player,
    ///     and without this rule an hour of it fills the whole list with one song and pushes out
    ///     everything that was actually listened to.
    ///   * **Survives a context change, a shuffle and a reboot.** Kept with the rest of the
    ///     playback state (`playback_encode`), because a "previously played" that empties every
    ///     time the device sleeps would be answering a different question from the one it asks.
    ///   * **Only an explicit CLEAR empties it** — the control on the section's own header. Not
    ///     the queue's CLEAR chip: one destroys what you asked to hear next, the other what you
    ///     already heard, and a single button for both would be the kind of overload this screen
    ///     has had cleaned out of it before.
    ///   * **Nothing else is ever recorded.** FM is not tracks, so it never reaches
    ///     `track_started` and never lands here.
    history: Vec<SongRow>,
    /// Transient bottom toast ("Added to queue — …"): text + frames left (fades via tick()).
    toast: String,
    toast_raw: bool,
    /// Dynamic notices retain their message key and owned parameters until each render.
    toast_template: Option<ToastTemplate>,
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
    lib_tab_zones: std::cell::RefCell<Vec<(Option<Tab>, f32, f32)>>,
    /// Object ids of the Up Next rows the last render actually drew, in drawn order. The window
    /// auto-scrolls to follow playback, so the renderer (which knows `np`) publishes this for the
    /// hit test instead of `tap` trying to recompute it.
    /// The context index the auto-follow last snapped to, so `render` can tell a track change from
    /// a redraw. Everything else Up Next needs now comes straight off `context`/`queue`, so no hit
    /// test depends on a frame having been drawn first.
    up_next_cur: Option<usize>,
    /// The same memo for the playing track's object id: an edit to the list (a shuffle, a row
    /// dropped in front of it) can change which track sits at an index without the index moving.
    up_next_id: Option<i64>,
    /// Auto power-off, in minutes (0 = off). Persisted. The shell reads it and owns the timer.
    auto_off_idx: usize,
    auto_off_min: u32,
    /// Genre picker scroll offset.
    genre_scroll_px: i32,
    /// Track-information rows, `(label, value)`, pushed by the shell whenever the track changes.
    /// Owned strings rather than a typed view: see `track_info` for why the shape is a list.
    /// Where we are in the folder tree: indices into `lib.folders`, root-first. Empty = the list
    /// of storage roots. A stack rather than a single index, because Back has to walk up and the
    /// parent link alone would not restore the scroll offset of each level.
    folder_stack: Vec<usize>,
    folder_scroll_px: i32,
    /// One saved scroll offset per level already descended, so coming back up puts the folder you
    /// went into back under your finger instead of at the top of a hundred-row list.
    folder_scroll_saved: Vec<i32>,
    /// Which SensMe channel is open (an index into `lib.channels`), or `None` for the channel list.
    /// One level rather than the folder tree's stack: channels do not nest.
    sensme_channel: Option<usize>,
    sensme_scroll_px: i32,
    /// Scroll of the Audiobooks list.
    books_scroll_px: i32,
    /// The channel LIST's scroll, kept while a channel is open so Back puts the channel you went
    /// into back under your finger — the folder tree's `folder_scroll_saved`, for one level.
    sensme_scroll_saved: i32,
    /// Is the opt-in `sensme` component installed? Set by the shell at startup from
    /// `/data/cinder/sensme_on`; without it the Menu has no SensMe row.
    sensme_enabled: bool,
    /// The opt-in `preview` component is installed: the Menu offers the Preview row.
    preview_enabled: bool,
    zappin_preview_ms: u32,
    zappin_active: bool,
    zappin_completed: bool,
    zappin_source: Option<crate::zappin::StartSource>,
    zappin_start_ms: Option<u64>,
    zappin_elapsed_ms: u32,
    zappin_length_ms: u32,
    zappin_current: u32,
    zappin_total: u32,
    /// SensMe ▸ "Follow the time of day": the grid's button plays the channel for the hour.
    sensme_follow: bool,
    /// The hour on the status-bar clock at the last render, for the time-of-day channel. `None`
    /// until a clock has been drawn (the host and tests have no shell).
    clock_hour: Option<u8>,
    track_info: Vec<(String, String)>,
    track_info_scroll_px: i32,
    /// Measured content height of the track-info list, refreshed on every paint of that screen.
    ///
    /// It cannot be computed where the rows are SET: the height depends on how many lines each
    /// value wraps to, wrapping needs font metrics, and `set_track_info` has no `FontSet` — only
    /// `render` does. Caching the render's own measurement is exact and costs nothing, and the
    /// ordering is safe: a screen has to be painted before a finger can drag it.
    track_info_h: i32,
    /// The "Add tracks" search text. Empty = the whole library.
    track_pick_query: String,
    /// Row heights from the last Track information frame, so a tap can resolve which row it hit
    /// without font metrics. See `track_info::row_heights`.
    track_info_rows: Vec<i32>,
    /// The playing song's lyrics, pushed by the shell on every track change. `None` = no `.lrc`.
    lyrics: Option<crate::lyrics::Lyrics>,
    lyrics_scroll_px: i32,
    /// Line heights from the last Lyrics frame — measured by the render for the same reason as
    /// `track_info_rows`: wrapping needs font metrics, and only `render` has them.
    lyrics_heights: Vec<i32>,
    /// The playback position the highlight follows, pushed by the shell's position tick.
    lyrics_pos_ms: u32,
    /// Does the view still follow the song? True on every entry; the user's own scroll clears it,
    /// the same rule as `queue_follow`.
    lyrics_follow: bool,
    /// Is the opt-in `search` component installed? Set once by the shell at startup; without it
    /// the Library header has no search button.
    search_enabled: bool,
    /// `hold_play_next` in cinder_advanced.conf: a hold on a SONG row is Play next. Folder rows
    /// answer a hold whatever this says (#56). The shell sets it before every hold.
    hold_play_next: bool,
    search_query: String,
    /// Matching songs as ABSOLUTE indices into `lib.songs`, like `track_pick_order`.
    search_order: Vec<crate::search::Hit>,
    search_scroll_px: i32,
    /// Up Next's "everything after the playing track" length, keyed on (list address, length,
    /// position). See `Screen::UpNext` in `render`.
    queue_after_memo: Option<((usize, usize, usize), Option<u32>)>,
    /// Memoised A-Z rail map, keyed on (tab, songs sort, albums order). See `az_present_memo`.
    az_memo: Option<((u8, usize, usize), [bool; 27])>,
    /// Each rail letter's offset, taken when the scrollbar is grabbed on a Library tab, so the drag
    /// can show which letter it is at (`library::az_bubble`). None when no drag or no rail.
    sbar_az: Option<[Option<i32>; 27]>,
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
            language: crate::i18n::Language::English,
            languages: vec![crate::i18n::Language::English],
            album_cover: None,
            night: false,
            accent: Accent::default(),
            palette: CINDER,
            palettes: Vec::new(),
            palette_active: None,
            palette_wanted: String::from(crate::palette::BUILTIN_ID),
            palette_skipped: Vec::new(),
            palette_sort: 0,
            palette_scroll_px: 0,
            language_scroll_px: 0,
            skin_names: Vec::new(),
            hp_eq: String::new(),
            skin_cur: String::new(),
            skins_scroll_px: 0,
            help_scroll_px: 0,
            palettes_stale: false,
            locked: true,
            playing: true,
            menu_idx: 0,
            lib_tab: Tab::Albums,
            lib_idx: 0,
            lib_scroll_px: 0,
            lib_band_hide: 0,
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
            artist_songs_open: false,
            artist_scroll_px: 0,
            swipe_row: None,
            swipe_live: false,
            queue_scroll_px: 0,
            row_drag: None,
            context: Vec::new(),
            context_idx: 0,
            pre_shuffle: None,
            rng: crate::shuffle::Rng::new(0x9E3779B97F4A7C15),
            queue_keep: false,
            hand_added: false,
            added_ids: Vec::new(),
            shuffle_on: false,
            sbar: None,
            fling_v: 0.0,
            volume: 15,
            vol_overlay: 0,
            volume_limit: false,
            winamp_skin: false,
            tape_loaded: false,
            wa_remaining: false,
            wa_title_shift: 0,
            wa_playlist: false,
            wa_pl_first: None,
            wa_vis: 0,
            wa_style: 0,
            ignore_the: false,
            lib_views: Default::default(),
            bt_volume: 15,
            bt_route: false,
            bt_connected: None,
            bt_debug_log: false,
            // ON by default since 2026-10-04 (the owner: every battery saver on, no edge case left
            // that drains a player nobody is using). Settings ▸ Bluetooth auto off turns it off.
            bt_idle_off: true,
            bt_link_known: false,
            bt_paired: Vec::new(),
            bt_found: Vec::new(),
            bt_paired_page: 0,
            bt_scanning: false,
            bt_prompt: None,
            rx_on: false,
            rx_codec_pref: 0,
            rx_phase: 0,
            rx_peer: String::new(),
            rx_codec: 0,
            rx_freq: 0,
            rx_bitrate: 0,
            bt_forget_armed: None,
            bt_connecting: None,
            bt_connecting_name: None,
            bt_connecting_ms: 0,
            busy_phase: 0.0,
            playing_hold_ms: 0,
            eq_bands: data::EQ_PRESETS[3].1, // "A1"
            eq_sel: 0,
            eq_preset: 3,
            bt_on: true,
            bt_codec: 0, // LDAC
            bt_aac: false,
            bt_sbc_xq: false,
            bt_ldac_quality: 0, // Auto
            bt_enhanced: true,
            bt_enhanced_supported: true,
            usb_dac_on: false,
            mono_shim: false,
            ambient_on: false,
            ambient_sound: crate::soundscape::DEFAULT_SOUND,
            ambient_alone: crate::soundscape::DEFAULT_ALONE,
            ambient_music: crate::soundscape::DEFAULT_MUSIC,
            ambient_route: crate::soundscape::Route::Off,
            ambient_sel: 0,
            usb_dac_fmt: None,
            airplay_meta: Default::default(),
            usb_dac_ll_avail: false,
            usb_dac_ll: false,
            usb_dac_ll_stats: None,
            bt_codec_negotiated: 0,
            bt_fine: 0, // OFF: it rides on the EQ, so it is a thing you opt into
            // NOT persisted, and deliberately: a trim is an attenuation the user cannot see in any
            // menu, and one restored at boot would be a device that is quietly 3 dB quiet with no
            // way to discover why.
            bt_trim: 0,
            viz_kind: 0,
            viz_size: 1, // VEIL on the cover page; the big spectrum has its own page
            viz_scale: 0,
            viz_range: 0,
            viz_response: 0,
            viz_interp: 0,
            viz_peak_hold: false,
            viz_window: 0, // AUTO — leave the service's own window alone until asked otherwise
            viz_bands: crate::vizcfg::BANDS_DEFAULT,
            viz_rate: 0, // 20 Hz, what the shell asked for before this was a setting
            viz_sel: 0,
            np_page: 0,
            settings_sel: 0,
            display_sel: 0,
            night_sched: false,
            night_from: crate::display::NIGHT_FROM_DEFAULT,
            night_until: crate::display::NIGHT_UNTIL_DEFAULT,
            night_in_window: None,
            home_screen: HOME_LIBRARY,
            home_last: None,
            browse_books: false,
            home_pending: false,
            volume_hud: 0,
            style: crate::style::Style::Cinder,
            battery_care: false,
            batt_pct: 0,
            // Empty, not "Unknown": the screen prints these verbatim, and an invented word would
            // look like a reading. Until the shell pushes real values there is nothing to say.
            batt_status: String::new(),
            batt_health: String::new(),
            batt_mv: crate::device::UNKNOWN,
            batt_chg_state: -1,
            batt_chg_fault: -1,
            batt_raw: String::new(),
            // UNKNOWN, not 0 — every one of these renders as a dash until the shell has read it,
            // because a zero clock or a zero temperature looks like a measurement.
            dev_temp_cpu: crate::device::UNKNOWN,
            dev_temp_pmic: crate::device::UNKNOWN,
            dev_temp_abb: crate::device::UNKNOWN,
            dev_cpu_khz: crate::device::UNKNOWN,
            dev_cpu_max_khz: crate::device::UNKNOWN,
            dev_cores_online: 0,
            dev_cores_total: 0,
            dev_governor: String::new(),
            dev_mem_total_kb: 0,
            dev_mem_avail_kb: 0,
            dev_music_total_mb: 0,
            dev_music_free_mb: crate::device::UNKNOWN,
            dev_data_free_mb: crate::device::UNKNOWN,
            dev_uptime_s: crate::device::UNKNOWN,
            dev_kernel: String::new(),
            dev_base_fw: String::new(),
            device_scroll_px: 0,
            storage_scroll_px: 0,
            display_scroll_px: 0,
            bt_codec_scroll_px: 0,
            menu_scroll_px: 0,
            rescanning: false,
            snd_dsee: false,
            snd_vinyl: false,
            snd_vpt: false,
            snd_vpt_mode: 0,
            snd_dc_type: 0,
            adv_source_direct: false,
            adv_clear_phase: false,
            adv_dsee_ai: false,
            adv_hp_linear: false,
            adv_dsee_custom: false,
            adv_dsee_mode: 0,
            adv_vinyl_type: 0,
            adv_tone: false,
            adv_clear_bass: 0,
            clear_bass_available: false,
            w1_chain: false,
            fm_khz: 97300,
            fm_playing: false,
            fm_stations: [0; crate::fm::PRESETS],
            fm_n_stations: 0,
            fm_scanning: false,
            fm_scan_pct: 0,
            fm_antenna: false,
            fm_bt_out: false,
            fm_rec_available: false,
            fm_rec_secs: -1,
            fm_signal: -1,
            fm_hw: false,
            fm_stereo: false,
            tone_bands: [0; crate::tone::BANDS],
            tone_sel: 0,
            dac_eq: [0; crate::dac_eq::BANDS],
            dac_eq_sel: 0,
            vol_curve: crate::vol_curve::STRAIGHT,
            vol_curve_on: false,
            vol_curve_ok: false,
            vol_curve_sel: 0,
            adv_sel: crate::advanced::ROW_VPT,
            adv_scroll_px: 0,
            snd_dc: false,
            snd_norm: false,
            snd_clear: false,
            snd_balance: crate::sound::BALANCE_CENTRE,
            setup_other: SoundSetup::default(),
            setup_idx: 0,
            presets: Vec::new(),
            preset_names: Vec::new(),
            presets_scroll_px: 0,
            preset_armed: None,
            album_sounds: Vec::new(),
            album_sound_key: None,
            album_sound_prev: None,
            profile_follow: 0,
            live_output: crate::profile::Output::Jack,
            profile_apply: false,
            jack_eq_dac: true,
            nc_plug: false,
            nc_ambient_ok: false,
            nc_mode: crate::nc::MODE_OFF,
            nc_model: crate::nc::MODEL_NW500N,
            sound_scroll_px: 0,
            clearaudio_ok: true,
            clock_epoch: 0,
            clock_fields: [2026, 1, 1, 0, 0],
            clock_sel: 0,
            alarm_on: false,
            alarm_min: crate::alarm::DEFAULT_MIN,
            alarm_sel: 0,
            wifi_status: crate::wifi::Status::default(),
            wifi_networks: Vec::new(),
            wifi_scanning: false,
            wifi_wps: false,
            wifi_join_target: None,
            wifi_join: None,
            airplay_note: String::new(),
            play_to_note: String::new(),
            drive_note: String::new(),
            drive_code: String::new(),
            wifi_lb_note: String::new(),
            wifi_update_note: String::new(),
            update_offer: String::new(),
            update_install_note: String::new(),
            update_running: false,
            radio_stations: Vec::new(),
            radio_status: crate::netradio::Status::default(),
            radio_scroll_px: 0,
            radio_pick: None,
            remote_on: false,
            remote_phones: 0,
            remote_code: String::new(),
            remote_port: 0,
            remote_dropped: false,
            speakers: Vec::new(),
            speakers_scanning: false,
            media_levels: Vec::new(),
            media_busy: false,
            media_note: String::new(),
            media_playing: String::new(),
            media_query: String::new(),
            radio_query: String::new(),
            radio_found: Vec::new(),
            radio_finding: false,
            radio_find_note: String::new(),
            cast_speaker: None,
            cast_line: String::new(),
            snd_ab_bypass: false,
            sound_sel: 0,
            storage: String::new(),
            sleep_idx: 0,
            boot_stock_armed: false,
            confirm: None,
            kb_text: String::new(),
            kb_shift: false,
            kb_page: 0,
            kb_purpose: KbPurpose::NewPlaylist,
            pick_track: 0,
            pick_many: Vec::new(),
            pick_many_name: String::new(),
            folder_sel: Vec::new(),
            glue_pairs: Default::default(),
            pick_scroll_px: 0,
            cover_scroll_px: 0,
            track_pick_scroll_px: 0,
            track_pick_order: Vec::new(),
            pl_edit: None,
            pl_edit_done: None,
            views: Vec::new(),
            view_edit: None,
            shuffle_by: crate::shuffle::ShuffleBy::Songs,
            list_shuffle_by: crate::shuffle::ShuffleBy::Songs,
            pending_play: None,
            pending_rest: Vec::new(),
            play_list: Vec::new(),
            liked_count: 0,
            settings_scroll_px: 0,
            settings_page: None,
            // 30 s by default. This was OFF, deliberately, because "a failed wake looks like a dead
            // device" — but wake is now proven three ways (touch, the Power button, any key), the
            // auto-off path leaves the touch controller POWERED precisely so a tap can wake it, and
            // a waking touch is consumed so it cannot also press whatever is underneath.
            //
            // The cost of leaving it off is the largest single power draw on the device running
            // forever: measured 2026-08-16 with the panel at full brightness 21 minutes after the
            // last input, because the owner had never found the setting. A default nobody discovers
            // is a default that has to be right.
            screen_off_idx: 2,
            screen_off_s: SCREEN_OFF_PRESETS[2],
            brightness: 4, // matches the shell's ~70% day default
            brightness_restore: 4,
            sleep_min: 0,
            stop_after: false,
            onboarding_page: 0,
            onboarding_seen: false,
            lib: Library::sample(),
            shelf_open: false,
            opts: crate::opt::Opts::default(),
            delete_ids: Vec::new(),
            np_swipe_up: crate::settings::SwipeTo::Off,
            np_swipe_down: crate::settings::SwipeTo::Off,
            np_button: crate::now_playing::NpButton::Bluetooth,
            np_from: Vec::new(),
            quick_open: false,
            pins: std::array::from_fn(|_| None),
            history: Vec::new(),
            mono: false,
            toast: String::new(),
            toast_raw: false,
            toast_template: None,
            toast_frames: 0,
            queue_anim_y: 0,
            queue_anim_frames: 0,
            scrub: Scrub::None,
            scrub_permille: 0,
            lib_tab_zones: std::cell::RefCell::new(Vec::new()),
            up_next_cur: None,
            up_next_id: None,
            queue_follow: true,
            // 30 MIN by default since 2026-10-04 (was OFF): a paused player with a dark screen ran
            // until the battery was flat. OFF is still the first preset and one tap away.
            auto_off_idx: 2,
            auto_off_min: AUTO_OFF_PRESETS[2],
            genre_scroll_px: 0,
            folder_stack: Vec::new(),
            folder_scroll_px: 0,
            folder_scroll_saved: Vec::new(),
            sensme_channel: None,
            sensme_scroll_px: 0,
            books_scroll_px: 0,
            sensme_scroll_saved: 0,
            sensme_enabled: false,
            preview_enabled: false,
            zappin_preview_ms: 4_000,
            zappin_active: false,
            zappin_completed: false,
            zappin_source: None,
            zappin_start_ms: None,
            zappin_elapsed_ms: 0,
            zappin_length_ms: 4_000,
            zappin_current: 0,
            zappin_total: 0,
            sensme_follow: false,
            clock_hour: None,
            track_info: Vec::new(),
            track_info_scroll_px: 0,
            track_info_h: 0,
            track_pick_query: String::new(),
            track_info_rows: Vec::new(),
            lyrics: None,
            lyrics_scroll_px: 0,
            lyrics_heights: Vec::new(),
            lyrics_pos_ms: 0,
            lyrics_follow: true,
            search_enabled: false,
            hold_play_next: false,
            search_query: String::new(),
            search_order: Vec::new(),
            search_scroll_px: 0,
            az_memo: None,
            sbar_az: None,
            queue_after_memo: None,
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
        App {
            stack: vec![Screen::NowPlaying],
            locked: false,
            ..Self::default()
        }
    }

    pub fn language(&self) -> crate::i18n::Language {
        self.language
    }

    /// Only an installed language: a settings file from another install cannot pick one.
    pub fn set_language(&mut self, language: crate::i18n::Language) {
        if self.languages.contains(&language) {
            self.language = language;
        }
    }

    /// The languages the installer put on this player. None means English; the language in use
    /// moves to the first of them if it is not one.
    pub fn set_languages(&mut self, languages: &[crate::i18n::Language]) {
        self.languages = crate::i18n::Language::ALL
            .into_iter()
            .filter(|l| languages.contains(l))
            .collect();
        if self.languages.is_empty() {
            self.languages.push(crate::i18n::Language::English);
        }
        if !self.languages.contains(&self.language) {
            self.language = self.languages[0];
        }
    }

    /// The Settings Language row is on screen: there is more than one language to switch between.
    fn settings_lang(&self) -> bool {
        self.languages.len() > 1
    }

    // ── test-only entry points ──────────────────────────────────────────────────────────────
    // The overflow audit (tests/ui_overflow.rs) has to reach every screen and overlay. These do
    // that without making `stack`, `shelf_open` or `confirm` public, which would let any caller
    // put the navigator in a state its own transitions can never produce.

    /// Open `s` as if it had been routed to. Screens that carry per-screen state get it set up
    /// here, so the audit renders them the way a user would actually see them.
    #[doc(hidden)]
    pub fn push_for_test(&mut self, s: Screen) {
        match s {
            Screen::Lock => {
                self.locked = true;
                self.stack = vec![Screen::NowPlaying];
                return;
            }
            Screen::Folders => {
                self.open_folders();
                return;
            }
            Screen::SensMe => {
                // The component gates the MENU ROW, not the screen; a test that asks for the
                // screen gets it, with the row that leads there present too.
                self.sensme_enabled = true;
                self.open_sensme();
                return;
            }
            Screen::PlaylistEdit => {
                // The editor opens on a playlist Cinder owns; give the sample library one.
                if let Some(p) = self.lib.playlists.iter_mut().find(|p| !p.smart) {
                    p.user = true;
                }
                self.playlist_view = self.lib.playlists.iter().position(|p| p.user).unwrap_or(0);
                self.stack.push(Screen::Playlist);
                self.open_playlist_edit();
                return;
            }
            Screen::PlaylistCover => {
                if let Some(p) = self.lib.playlists.iter_mut().find(|p| !p.smart) {
                    p.user = true;
                }
                self.playlist_view = self.lib.playlists.iter().position(|p| p.user).unwrap_or(0);
                self.stack.push(Screen::Playlist);
                self.open_cover_pick();
                return;
            }
            Screen::ViewEdit => {
                // A view that exists, so the Delete row is drawn, under a name long enough to test
                // the Name row's fit.
                self.views = vec![crate::views::SavedView {
                    name: "Every Song I Could Not Skip This Winter, vol. 2"
                        .chars()
                        .take(48)
                        .collect(),
                    min_rating: 4,
                    ..Default::default()
                }];
                self.rebuild_smart(None);
                self.open_view_edit(Some(0));
                return;
            }
            Screen::TrackInfo => {
                // Needs rows, or it renders an empty page and audits nothing.
                self.track_info = vec![
                    (crate::i18n::text("Rating").into(), "-".into()),
                    (crate::i18n::text("Title").into(), "Sinfonia concertante for Violin, Viola and Orchestra in E-flat major, K. 364".into()),
                    (crate::i18n::text("Artist").into(), "Королевский филармонический оркестр / 東京都交響楽団".into()),
                    (crate::i18n::text("Path").into(), "/contents_ext/Music/Classical/Mozart/Sinfonia concertante K364/03 - III. Presto.flac".into()),
                ];
            }
            Screen::Lyrics => {
                // Enough lines to scroll, long ones that wrap, three scripts, and a current line.
                let lines = (0..24u32)
                    .map(|i| crate::lyrics::Line {
                        at_ms: Some(i * 4000),
                        text: match i % 3 {
                            0 => "A line long enough that it cannot fit the page and has to wrap onto the next row".into(),
                            1 => "Королевский филармонический оркестр".into(),
                            _ => "東京都交響楽団".into(),
                        },
                    })
                    .collect();
                self.lyrics = Some(crate::lyrics::Lyrics { lines });
                self.lyrics_pos_ms = 9_000;
            }
            Screen::Search => {
                // Results rather than the empty prompt: the rows are what can overflow.
                self.search_query = "a".into();
                self.search_rebuild();
            }
            Screen::TrackPick => {
                // Rows, through the real open path: pushed bare, the picker had none, so its row
                // text was never audited.
                self.open_track_pick();
                return;
            }
            _ => {}
        }
        self.stack.push(s);
    }

    #[doc(hidden)]
    pub fn set_track_info_rows_for_test(&mut self, h: Vec<i32>) {
        self.track_info_rows = h;
    }
    /// The keyboard's header, which must name the job it is doing.
    #[doc(hidden)]
    pub fn kb_title_for_test(&self) -> String {
        match self.kb_purpose {
            KbPurpose::Rename(_) => crate::i18n::text("Rename playlist").to_string(),
            KbPurpose::SaveQueue => crate::i18n::text("Save Up Next").to_string(),
            KbPurpose::ViewName => crate::i18n::text("Name it").to_string(),
            KbPurpose::TrackSearch => {
                if self.kb_text.trim().is_empty() {
                    crate::i18n::text("Find a song").to_string()
                } else {
                    crate::ui_format!("{} matching", self.track_pick_order.len())
                }
            }
            _ => crate::i18n::text("New playlist").to_string(),
        }
    }
    #[doc(hidden)]
    pub fn track_pick_len_for_test(&self) -> usize {
        self.track_pick_order.len()
    }
    #[doc(hidden)]
    pub fn album_view_for_test(&self) -> usize {
        self.album_view
    }
    #[doc(hidden)]
    #[doc(hidden)]
    pub fn lib_scroll_px_for_test(&self) -> i32 {
        self.lib_scroll_px
    }

    pub fn set_night_for_test(&mut self, n: bool) {
        self.night = n;
    }

    pub fn open_shelf_for_test(&mut self) {
        self.open_shelf();
    }

    /// The Library on `tab`, as a player that starts where it was left would open it.
    #[doc(hidden)]
    pub fn open_library_for_test(&mut self, tab: library::Tab) {
        self.stack = vec![Screen::Library];
        self.lib_tab = tab;
    }

    #[doc(hidden)]
    /// Type into the keyboard from a test, as a user would key by key.
    #[doc(hidden)]
    pub fn type_for_test(&mut self, s: &str) {
        for ch in s.chars() {
            let key = if ch == ' ' {
                crate::keyboard::Key::Space
            } else {
                crate::keyboard::Key::Char(ch)
            };
            crate::keyboard::apply(
                key,
                &mut self.kb_text,
                &mut self.kb_shift,
                &mut self.kb_page,
            );
        }
        self.kb_after_key();
    }

    /// Replace what the keyboard holds, as if typed over its seed.
    #[doc(hidden)]
    pub fn kb_text_for_test(&mut self, s: &str) {
        self.kb_text = s.to_string();
        self.kb_after_key();
    }

    pub fn open_confirm_for_test(&mut self, ask: crate::confirm::Ask) {
        self.confirm = Some(ask);
    }

    /// Settings ▸ Reset settings, confirmed. Every preference goes back to its default.
    ///
    /// It rebuilds from `App::default()` and carries a SHORT list of things forward, rather than
    /// assigning defaults field by field. That is the whole point: this file has ~95 fields and
    /// gains one most weeks, so a hand-written reset is a list that silently stops being complete.
    /// Inverting it means a new preference is reset for free and only genuinely non-preference
    /// state has to be named here.
    ///
    /// What survives, and why:
    ///   * the LIBRARY — a reset of preferences must not look like a wipe (its filters do reset;
    ///     a filter is a preference and an invisible one is exactly what a reset is for);
    ///   * WHAT IS PLAYING — context, queue, position and the un-shuffle order. Silencing the
    ///     music to change a theme default would be its own bug;
    ///   * the SHELF PINS — bookmarks the user placed by hand, i.e. their data, not a setting;
    ///   * the HOLD switch — hardware truth. Resetting `locked` to its default would leave the
    ///     UI unlocked with the physical switch still engaged, which is precisely the desync
    ///     fixed by reading `EVIOCGKEY` at boot;
    ///   * the SCREEN we are on, so the confirmation lands back on Settings.
    pub fn reset_settings(&mut self) -> Vec<Action> {
        let mut lib = std::mem::take(&mut self.lib);
        lib.filter_genre = None;
        lib.filter_hires = false;
        // The palette FILES are not a preference — they are what is in the folder, like the library.
        // The choice between them resets; the list of them stays.
        let palettes = std::mem::take(&mut self.palettes);
        let palette_skipped = std::mem::take(&mut self.palette_skipped);
        let keep = (
            std::mem::take(&mut self.context),
            self.context_idx,
            (self.hand_added, std::mem::take(&mut self.added_ids)),
            self.pre_shuffle.take(),
            std::mem::take(&mut self.pins),
            self.locked,
            self.playing,
            // Saved views are things the owner MADE, like playlists and pins — not preferences.
            std::mem::take(&mut self.views),
        );
        *self = App {
            lib,
            palettes,
            palette_skipped,
            context: keep.0,
            context_idx: keep.1,
            hand_added: keep.2 .0,
            added_ids: keep.2 .1,
            pre_shuffle: keep.3,
            pins: keep.4,
            locked: keep.5,
            playing: keep.6,
            views: keep.7,
            stack: vec![Screen::Settings],
            settings_sel: crate::settings::ROW_RESET,
            // The intro is not shown again: it was completed once, and re-running it would make a
            // settings reset look like a factory reset.
            onboarding_seen: true,
            ..App::default()
        };
        // The kept library is still sorted under the old preference; put it back in default order.
        self.set_ignore_the(false);
        // And the switches' knock-on state (the library's FOLDERS link) goes back off with them.
        for o in Opt::ALL {
            self.set_opt(o, false);
        }
        self.lib.views = self.lib_views;
        self.notify(crate::i18n::text("Settings reset"));
        vec![Action::SettingsReset]
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
        self.go(if held {
            Screen::Lock
        } else {
            Screen::NowPlaying
        });
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
    /// Usually it is just `PlayIndex`. When the user has added to Up Next by hand and some of it is
    /// still to come, it is a question first: playing something new either replaces that list or
    /// plays it afterwards, and silently picking either one is wrong. Apple asks the same thing.
    ///
    /// A funnel rather than a check at each call site: `PlayIndex` is emitted from seven places
    /// (song rows, album rows, search, playlists, shuffle, the shelf), and a prompt that only some
    /// of them respect is worse than no prompt — it would look like the queue is discarded at
    /// random depending on which screen you started from.
    fn start_play(&mut self, id: i64) -> Vec<Action> {
        self.start_play_action(Action::PlayIndex(id))
    }

    /// Play row `r` of an album page: its file, from the CUE track's start when it is one.
    fn play_album_row(&mut self, album: &crate::model::AlbumRow, r: usize) -> Vec<Action> {
        match album.row_file(r) {
            Some((s, 0)) => self.start_play(s.object_id),
            Some((s, ms)) => self.start_play_action(Action::PlayIndexAt(
                s.object_id,
                ms.clamp(0, u32::MAX as i64) as u32,
            )),
            None => vec![],
        }
    }

    /// The same funnel for a play action that already knows its context (a playlist member, say).
    /// `start_play` is the object-id shorthand for it.
    fn start_play_action(&mut self, act: Action) -> Vec<Action> {
        // THE PLAYING SONG, TAPPED AGAIN in an album, a playlist or the Songs list. Starting that
        // list from it would restart the song mid-play; ask instead, and offer to queue the rest
        // of that list behind it (owner, 2026-11-16: "no matter where it came from"). Not asked
        // when what follows is already that list: then the tap means what it always meant.
        if let Some((rest, ask)) = self.rest_after_playing(&act) {
            let upcoming = self.context.get(self.context_idx + 1..).unwrap_or(&[]);
            // The list's next few songs are what comes next already (more may follow them).
            let n = rest.len().min(3);
            let same = n > 0
                && upcoming.len() >= n
                && upcoming
                    .iter()
                    .zip(&rest)
                    .take(n)
                    .all(|(a, b)| a.object_id == b.object_id);
            if !same {
                self.pending_play = Some(act);
                self.pending_rest = rest;
                self.confirm = Some(ask);
                return vec![];
            }
        }
        self.start_play_now(act)
    }

    /// What would play after the tapped song if `act` started its list, when that song is the one
    /// playing: the rest of the playlist, the album, or the list on screen. `None` for every other
    /// play (a different song, a book, a channel, a shuffle, a CUE track).
    fn rest_after_playing(
        &self,
        act: &Action,
    ) -> Option<(Vec<crate::model::SongRow>, crate::confirm::Ask)> {
        let playing = self.playing_object_id()?;
        match act {
            Action::PlayPlaylistAt(id, i) => {
                let pl = self.lib.playlists.iter().find(|p| p.id == *id)?;
                let i = *i as usize;
                (pl.track_list.get(i)?.object_id == playing).then(|| {
                    (
                        pl.track_list[i + 1..].to_vec(),
                        crate::confirm::Ask::SongPlayingPlaylist,
                    )
                })
            }
            Action::PlayListAt(n) => {
                let n = *n;
                (*self.play_list.get(n)? == playing).then(|| {
                    let rest = self.play_list[n + 1..]
                        .iter()
                        .filter_map(|id| {
                            self.lib.songs.iter().find(|s| s.object_id == *id).cloned()
                        })
                        .collect();
                    (rest, crate::confirm::Ask::SongPlayingList)
                })
            }
            Action::PlayIndex(id) if *id == playing => self
                .lib
                .album_groups
                .iter()
                .flat_map(|g| &g.albums)
                .find_map(|al| {
                    let p = al.track_list.iter().position(|s| s.object_id == *id)?;
                    Some((
                        al.track_list[p + 1..].to_vec(),
                        crate::confirm::Ask::SongPlayingAlbum,
                    ))
                }),
            _ => None,
        }
    }

    /// The play funnel past the "This is the current song" question: the Up Next question, then play.
    fn start_play_now(&mut self, act: Action) -> Vec<Action> {
        // Only a HAND-BUILT list is worth interrupting for: an album the user merely started is
        // not work they would lose by starting another.
        if !self.hand_added || !self.added_still_to_come() {
            return vec![act];
        }
        self.pending_play = Some(act);
        self.confirm = Some(crate::confirm::Ask::QueueOnPlay);
        vec![]
    }

    /// Play `ids` — the list on screen, in the order it is drawn — from row `n`, through the same
    /// funnel as every other play (so a hand-built Up Next still gets its question).
    fn start_play_list(&mut self, ids: Vec<i64>, n: usize) -> Vec<Action> {
        if n >= ids.len() {
            return vec![];
        }
        self.play_list = ids;
        self.start_play_action(Action::PlayListAt(n))
    }

    /// What the last `Action::ShuffleList` should deal: the smart playlist's own mode, or the
    /// setting. Read by the shell next to `take_play_list`.
    pub fn list_shuffle_by(&self) -> crate::shuffle::ShuffleBy {
        self.list_shuffle_by
    }

    /// The ids behind the last `Action::PlayListAt`, taken so they cannot be played twice.
    pub fn take_play_list(&mut self) -> Vec<i64> {
        std::mem::take(&mut self.play_list)
    }

    /// The Songs tab as drawn — current sort, current filter — as object ids.
    fn songs_list_ids(&self) -> Vec<i64> {
        library::song_order(&self.lib, self.lib_sort)
            .into_iter()
            .filter_map(|i| self.lib.songs.get(i).map(|s| s.object_id))
            .collect()
    }

    /// Test helper: drive the play funnel (`start_play`) directly, without a hit test.
    #[cfg(test)]
    fn tap_for_test_play(&mut self, id: i64) -> Vec<Action> {
        self.start_play(id)
    }

    /// Test helper: add one track to Up Next by hand — which is what the replace prompt is about.
    /// Starts a one-track list first if nothing is playing, so the pick is still to come.
    #[cfg(test)]
    fn queue_push_for_test(&mut self) {
        if self.context.is_empty() {
            self.set_play_context(
                vec![SongRow {
                    title: crate::i18n::text("NOW").into(),
                    object_id: -1,
                    ..Default::default()
                }],
                0,
            );
        }
        let _ = self.enqueue_at(crate::model::SongRow::default(), 0, QueueAt::Later);
    }

    /// Is a modal dialog currently up? The shell uses this to decide whether the screen-blank
    /// timer should keep running (it must not blank a "Power off?" prompt out from under a finger).
    pub fn modal_open(&self) -> bool {
        self.confirm.is_some()
    }

    /// The screen currently on top of the route stack.
    /// TRUE when the screen showing right now can display album artwork, so a newly decoded cover
    /// is worth a repaint.
    ///
    /// The art-cache builder decodes ~340 covers on a first boot and used to set `dirty` after
    /// every one, unconditionally — ~340 forced full-screen rasters spread over the couple of
    /// minutes the builder runs, most of them redrawing a byte-identical screen because the user
    /// was in Settings or on Bluetooth (audit B11). It costs nothing while the panel is dark, so
    /// it bit during exactly the window a new user is poking at the device.
    ///
    /// Listed positively — a new screen is not assumed to want repaints — but getting it wrong is
    /// cheap in the safe direction: the renderer forces a full repaint every 5 s regardless, so the
    /// worst case for a missing entry is a cover that appears up to five seconds late.
    /// `NowPlaying` and `Menu` are included deliberately: both can draw a cover, and a stale cover
    /// on the screen the user is staring at is the one case worth spending a repaint to avoid.
    pub fn shows_album_art(&self) -> bool {
        matches!(
            self.current(),
            Screen::NowPlaying
                | Screen::Menu
                | Screen::Library
                | Screen::Album
                | Screen::Artist
                | Screen::Playlist
                | Screen::UpNext
                | Screen::GenreFilter
                | Screen::Folders
                | Screen::SensMe
                | Screen::TrackInfo
                | Screen::Lyrics
                | Screen::Search
                | Screen::TrackPick
                | Screen::Shelf
        )
    }

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
    fn activate_menu(&mut self, row: usize) -> Vec<Action> {
        let Some((target, _, _)) = self.menu_visible().get(row).map(|entry| **entry) else {
            return vec![];
        };
        match target {
            Screen::NowPlaying => self.go(Screen::NowPlaying),
            Screen::Zappin => self.push(Screen::Zappin),
            Screen::Help => {
                self.help_scroll_px = 0;
                self.push(Screen::Help);
            }
            // Not a bare push: entering the tree has to reset the descent stack and skip the
            // single-root level, which is what open_folders is for.
            Screen::Folders => self.open_folders(),
            // Same reason as Folders: entering has to reset which level is open, or the screen
            // comes back on the channel you were last in with the list's scroll under it.
            Screen::SensMe => self.open_sensme(),
            target => self.push(target),
        }
        vec![]
    }

    /// Open the Shelf bottom-sheet over the current place. Entered from the Menu, so pop the Menu
    /// first — the sheet should overlay the real screen beneath it, not the Menu.
    pub fn open_shelf(&mut self) {
        if self.current() == Screen::Menu {
            self.pop();
        }
        self.shelf_open = true;
    }

    /// Open the Shelf from an upward swipe off the bottom edge of the screen.
    ///
    /// A separate entry point from `open_shelf` because a GESTURE has to be able to decline. The
    /// Shelf is a global overlay, but it is wrong in three places: over the lock screen (nothing
    /// should be reachable before unlocking), during onboarding (the pager owns vertical space and
    /// the user has nowhere to go yet), and when it is already open (a second swipe should not
    /// re-open what it might have been trying to dismiss). Returns whether it took the gesture, so
    /// the shell can fall back to a scroll if it did not.
    pub fn shelf_swipe_open(&mut self) -> bool {
        if self.locked
            || self.shelf_open
            || self.quick_open
            || self.current() == Screen::Onboarding
            || self.landscape()
        {
            return false;
        }
        self.open_shelf();
        true
    }

    /// Whether the Shelf overlay is showing (the shell can use this, e.g. to keep painting).
    pub fn shelf_is_open(&self) -> bool {
        self.shelf_open
    }

    /// An overlay owns the gestures: the Shelf or the pull-down panel. The guards that used to
    /// read `shelf_open` alone read this, so the panel is as modal as the Shelf is.
    fn overlay_open(&self) -> bool {
        self.shelf_open || self.quick_open
    }

    /// The Bluetooth screens' view; `fine_volume` is the codec page's Fine volume value.
    fn bt_view<'a>(&'a self, fine_volume: &'a str) -> Bt<'a> {
        Bt {
            fine_volume,
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
            aac: self.bt_aac,
            xq: self.bt_sbc_xq,
            ldac_quality: self.bt_ldac_quality,
            link_codec: self.bt_link_codec_raw(),
            enhanced: self.bt_enhanced,
            enhanced_supported: self.bt_enhanced_supported,
            connecting: self.bt_connecting.is_some(),
            busy_phase: self.busy_phase,
            paired: &self.bt_paired,
            debug_log: self.bt_debug_log,
        }
    }

    /// The Sound and Advanced screens' VPT and DC Phase values.
    fn vpt_label(&self) -> &'static str {
        if self.snd_vpt {
            VPT_MODES[self.snd_vpt_mode.min(VPT_MODES.len() - 1)]
        } else {
            "Off"
        }
    }
    fn dc_label(&self) -> &'static str {
        if self.snd_dc {
            DC_PHASE_TYPES[self.snd_dc_type.min(DC_PHASE_TYPES.len() - 1)]
        } else {
            "Off"
        }
    }

    /// Is opt-in switch `o` on (`opt.rs`)?
    pub fn opt(&self, o: Opt) -> bool {
        self.opts.on(o)
    }

    /// Set switch `o`, and whatever has to follow it at once.
    pub fn set_opt(&mut self, o: Opt, on: bool) {
        self.opts.set(o, on);
        match o {
            Opt::Quick if !on => self.quick_open = false,
            Opt::FoldersTab => self.lib.folders_tab = on,
            Opt::NpBack => crate::now_playing::set_back(on),
            _ => {}
        }
    }

    /// ReplayGain as an index into `advanced::REPLAYGAIN`: 0 off, 1 track, 2 album.
    pub fn replaygain_mode(&self) -> usize {
        match (self.opt(Opt::ReplayGain), self.opt(Opt::RgAlbum)) {
            (false, _) => 0,
            (true, false) => 1,
            (true, true) => 2,
        }
    }

    /// Settings ▸ Power ▸ Alarm: its time in minutes after midnight when on, for the shell.
    pub fn alarm(&self) -> Option<u16> {
        self.alarm_on.then_some(self.alarm_min)
    }

    /// On or off, and the time, as the settings file keeps them.
    pub fn alarm_saved(&self) -> (bool, u16) {
        (self.alarm_on, self.alarm_min)
    }

    /// From the settings file: on or off, and the time.
    pub fn set_alarm(&mut self, on: bool, min: u16) {
        self.alarm_on = on;
        self.alarm_min = min % (24 * 60);
    }

    /// Line out holds the headphone level: say so instead of moving it.
    fn line_out_holds(&mut self) -> bool {
        if !self.opt(Opt::LineOut) {
            return false;
        }
        self.notify(crate::i18n::text("Line out: the volume is fixed"));
        true
    }

    /// The level line out drops to when the plug is pulled: quiet in earphones on any curve.
    pub const LINE_OUT_SAFE: u8 = 30;

    /// The headphone jack changed (in or out). With line out on, it ends: the next thing in the
    /// jack may be earphones. The level drops to [`Self::LINE_OUT_SAFE`] if it was above. True when
    /// the level changed, so the shell writes it to the codec.
    pub fn line_out_jack_change(&mut self) -> bool {
        if !self.opt(Opt::LineOut) {
            return false;
        }
        self.set_opt(Opt::LineOut, false);
        self.notify(crate::i18n::text("Line out off: the plug was pulled"));
        let was = self.volume;
        self.volume = self.volume.min(Self::LINE_OUT_SAFE);
        self.volume != was
    }

    /// Flip switch `o`. Returns its new state.
    pub fn toggle_opt(&mut self, o: Opt) -> bool {
        let on = !self.opt(o);
        self.set_opt(o, on);
        on
    }

    /// What a swipe up and a swipe down on Now Playing open, for the settings file.
    pub fn np_swipes(&self) -> (crate::settings::SwipeTo, crate::settings::SwipeTo) {
        (self.np_swipe_up, self.np_swipe_down)
    }

    pub fn set_np_swipes(&mut self, up: crate::settings::SwipeTo, down: crate::settings::SwipeTo) {
        self.np_swipe_up = up;
        self.np_swipe_down = down;
    }

    /// A vertical swipe on Now Playing ended, `dy` px down (negative = up). Opens what Settings ▸
    /// Swipe up / Swipe down picked, and says whether it took the gesture. Now Playing has no
    /// vertical drag of its own, so with both Off nothing changes; the Shelf's bottom edge and the
    /// pull-down panel's status bar claim their contacts before this is asked (cinder-home).
    pub fn np_swipe(&mut self, dy: i32) -> bool {
        use crate::settings::SwipeTo;
        const TRAVEL: i32 = 80;
        if self.current() != Screen::NowPlaying
            || self.locked
            || self.overlay_open()
            || self.landscape()
            || dy.abs() < TRAVEL
        {
            return false;
        }
        match if dy < 0 {
            self.np_swipe_up
        } else {
            self.np_swipe_down
        } {
            SwipeTo::Off => return false,
            SwipeTo::Menu => self.push(Screen::Menu),
            SwipeTo::Sound => self.push(Screen::Sound),
            SwipeTo::Eq => self.push(Screen::Eq),
            SwipeTo::UpNext if self.zappin_active => self.push(Screen::Zappin),
            SwipeTo::UpNext => self.push(Screen::UpNext),
            SwipeTo::Bluetooth => self.push(Screen::Bluetooth),
            SwipeTo::Library => self.open_browse(),
            SwipeTo::Folders => self.open_folders(),
        }
        true
    }

    /// Night's schedule, for the settings file: on, starts, ends (minutes after midnight).
    pub fn night_schedule(&self) -> (bool, u16, u16) {
        (self.night_sched, self.night_from, self.night_until)
    }

    pub fn set_night_schedule(&mut self, on: bool, from: u16, until: u16) {
        self.night_sched = on;
        crate::display::set_schedule_shown(on);
        self.night_from = from.min(24 * 60 - 1);
        self.night_until = until.min(24 * 60 - 1);
        self.night_in_window = None;
    }

    /// Display ▸ Night's chip: 0 Off, 1 On, 2 Auto (the schedule).
    fn night_mode(&self) -> usize {
        if self.night_sched {
            2
        } else {
            usize::from(self.night)
        }
    }

    /// Pick Off, On or Auto. Auto acts on the next clock tick, within a second.
    fn set_night_mode(&mut self, mode: usize) -> Vec<Action> {
        self.night_sched = mode == 2;
        self.night_in_window = None;
        crate::display::set_schedule_shown(self.night_sched);
        let want = mode == 1;
        if self.night_sched || self.night == want {
            return vec![];
        }
        self.night = want;
        vec![Action::ThemeChanged(want)]
    }

    /// The clock reached `minute` (after midnight, local). With the schedule on, Night follows it
    /// at the two edges only, as a phone's night mode does: on at Starts, off at Ends, and a hand
    /// on the switch in between holds until the next edge. The first tick after boot, or after the
    /// schedule changed, counts as an edge, so the player wakes into the right one.
    pub fn night_schedule_tick(&mut self, minute: u16) -> Vec<Action> {
        if !self.night_sched {
            self.night_in_window = None;
            return vec![];
        }
        let inside = crate::display::in_night_window(minute, self.night_from, self.night_until);
        if self.night_in_window == Some(inside) {
            return vec![];
        }
        self.night_in_window = Some(inside);
        if self.night == inside {
            return vec![];
        }
        self.night = inside;
        vec![Action::ThemeChanged(inside)]
    }

    /// A "Make it yours" row in the introduction: the same switch as its Settings row.
    fn onboarding_choice(&mut self, choice: crate::onboarding::Choice) -> Vec<Action> {
        use crate::onboarding::Choice;
        match choice {
            Choice::Big => {
                self.set_big_view(!crate::scale::large());
                vec![Action::UiScaleChanged]
            }
            Choice::Bar => {
                crate::scale::set_big_bar(!crate::scale::big_bar_set());
                vec![Action::UiScaleChanged]
            }
            Choice::Icons => {
                crate::scale::set_big_icons(!crate::scale::big_icons_set());
                vec![Action::UiScaleChanged]
            }
            Choice::Quick => {
                self.toggle_opt(Opt::Quick);
                vec![]
            }
            Choice::Folders => {
                self.toggle_opt(Opt::FoldersTab);
                vec![]
            }
        }
    }

    /// Now Playing's third toolbar button, for the settings file.
    pub fn np_button(&self) -> crate::now_playing::NpButton {
        self.np_button
    }

    pub fn set_np_button(&mut self, b: crate::now_playing::NpButton) {
        self.np_button = b;
    }

    /// The files a confirmed delete is for, once (`Action::DeleteMusic`).
    pub fn take_delete(&mut self) -> Vec<i64> {
        std::mem::take(&mut self.delete_ids)
    }

    /// Ask before deleting `ids`. A CUE album is ONE file, so a song of one cannot go on its own:
    /// the question becomes the album's.
    fn ask_delete(&mut self, ids: Vec<i64>, album: bool) {
        let cue = (!album)
            .then(|| {
                let id = *ids.first()?;
                self.lib
                    .albums_flat()
                    .into_iter()
                    .find(|a| !a.cue.is_empty() && a.track_list.iter().any(|t| t.object_id == id))
                    .map(|a| {
                        a.track_list
                            .iter()
                            .map(|t| t.object_id)
                            .collect::<Vec<i64>>()
                    })
            })
            .flatten();
        if ids.is_empty() {
            return;
        }
        self.confirm = Some(if album || cue.is_some() {
            crate::confirm::Ask::DeleteAlbum
        } else {
            crate::confirm::Ask::DeleteTrack
        });
        self.delete_ids = cue.unwrap_or(ids);
    }

    /// The delete question was answered yes. The page goes (what it shows is about to), Up Next
    /// loses what is still to come of it, and a song playing from it is skipped.
    fn delete_confirmed(&mut self) -> Vec<Action> {
        if self.delete_ids.is_empty() {
            return vec![];
        }
        self.pop();
        let ids = self.delete_ids.clone();
        let mut out = vec![Action::DeleteMusic];
        let before = self.context.len();
        for i in (self.context_idx + 1..self.context.len()).rev() {
            if ids.contains(&self.context[i].object_id) {
                self.context.remove(i);
            }
        }
        if let Some(pre) = self.pre_shuffle.as_mut() {
            pre.retain(|p| !ids.contains(p));
        }
        if self.context.len() != before {
            out.push(Action::QueueChanged);
        }
        if self.playing_object_id().is_some_and(|p| ids.contains(&p)) {
            out.push(Action::Next);
        }
        out
    }

    pub fn quick_is_open(&self) -> bool {
        self.quick_open
    }

    /// Does a vertical drag that STARTED at `(x, y)` belong to the pull-down panel? Asked by the
    /// shell once per contact, on the start point, before the list gets the drag.
    ///
    /// No unless the panel is switched on, the drag began in the status bar, and nothing modal is
    /// up. With the panel off the answer is always no, so the contact scrolls exactly as it did
    /// before the panel existed.
    pub fn quick_pull_begin(&self, _x: i32, y: i32) -> bool {
        self.opt(Opt::Quick)
            && (0..crate::chrome::STATUS_H).contains(&y)
            && self.quick_allowed()
            && !self.landscape()
    }

    /// Now Playing is a full-panel view (a Winamp skin, or a cassette tape on its side): the
    /// panel's edges and the status strip belong to the view, which has no status bar of its own.
    pub fn landscape(&self) -> bool {
        self.current() == Screen::NowPlaying && (self.winamp_view() || self.tape_view())
    }

    /// The pull travelled far enough: open the panel. False if something modal came up meanwhile.
    pub fn quick_pull_open(&mut self) -> bool {
        if !self.opt(Opt::Quick) || !self.quick_allowed() {
            return false;
        }
        self.quick_open = true;
        true
    }

    fn quick_allowed(&self) -> bool {
        !self.locked
            && !self.overlay_open()
            && self.confirm.is_none()
            && !matches!(
                self.current(),
                Screen::Onboarding | Screen::UsbStorage | Screen::Lock
            )
    }

    /// A tap while the panel is open. Every control is an action that already exists.
    fn quick_tap(&mut self, x: i32, y: i32) -> Vec<Action> {
        use crate::quick::{QuickHit, SLEEP_PRESETS};
        match crate::quick::hit(x, y) {
            QuickHit::Brightness(level) => {
                self.brightness = level;
                self.brightness_restore = level;
                vec![Action::BrightnessChanged(level)]
            }
            QuickHit::Bluetooth => {
                self.bt_on = !self.bt_on;
                vec![Action::BtToggle(self.bt_on)]
            }
            QuickHit::BluetoothPage => {
                self.quick_open = false;
                self.push(Screen::Bluetooth);
                vec![]
            }
            // As Settings ▸ Wi-Fi's own switch: busy until the shell's status says what happened.
            QuickHit::Wifi => {
                use crate::wifi::State;
                if matches!(self.wifi_status.state, State::Busy) {
                    return vec![];
                }
                let on = !matches!(self.wifi_status.state, State::Off);
                self.wifi_status.state = State::Busy;
                self.wifi_status.note.clear();
                crate::wifi::note_state(State::Busy);
                if on {
                    self.wifi_networks.clear();
                }
                vec![Action::WifiPower(!on)]
            }
            QuickHit::WifiPage => {
                self.quick_open = false;
                self.push(Screen::Wifi);
                vec![]
            }
            QuickHit::Night => {
                self.night = !self.night;
                vec![Action::ThemeChanged(self.night)]
            }
            QuickHit::Sleep(i) => self.set_sleep_choice(i.min(SLEEP_PRESETS.len() - 1)),
            QuickHit::Sheet => vec![],
            QuickHit::Outside => {
                self.quick_open = false;
                vec![]
            }
        }
    }

    /// A short title/subtitle for the current place — used by the Shelf's "this place" row and when
    /// pinning. Derived from the live nav state (no shell round-trip needed).
    fn place_label(&self) -> (String, String) {
        match self.current() {
            Screen::NowPlaying => (
                crate::i18n::text("Now Playing").into(),
                crate::i18n::text("Current track").into(),
            ),
            // ALBUMS TAB: name the OPEN ALBUM, not the tab. The Albums tab is an accordion — the
            // only way to "be at" an album without leaving the list is to have it expanded — so a
            // pin taken there used to read "Library / Albums" and was indistinguishable from any
            // other Albums pin. Naming the album is what makes pinning one from the list useful,
            // and `restore_pin` turns it into a jump-to. (Tapping the artwork still opens the full
            // Album screen, which was always pinnable; this covers the drop-down.)
            Screen::Library => match (self.lib_tab, self.album_expanded) {
                (Tab::Albums, Some(flat)) => match self.lib.albums_flat().get(flat) {
                    Some(al) => (al.name.clone(), al.artist.clone()),
                    None => (
                        crate::i18n::text("Library").into(),
                        tab_name(self.lib_tab).into(),
                    ),
                },
                _ => (
                    crate::i18n::text("Library").into(),
                    tab_name(self.lib_tab).into(),
                ),
            },
            Screen::Album => match self.lib.albums_flat().get(self.album_view) {
                Some(al) => (al.name.clone(), al.artist.clone()),
                None => (crate::i18n::text("Album").into(), String::new()),
            },
            Screen::Artist => match self.lib.artists.get(self.artist_view) {
                Some(ar) => (
                    ar.name.clone(),
                    crate::ui_format!("{} albums · {} tracks", ar.albums, ar.tracks),
                ),
                None => (crate::i18n::text("Artist").into(), String::new()),
            },
            Screen::Playlist => match self.playlist_row() {
                Some(p) => (
                    p.name.clone(),
                    crate::ui_format!("{} tracks", p.track_list.len()),
                ),
                None => (crate::i18n::text("Playlist").into(), String::new()),
            },
            Screen::Books => (
                crate::i18n::text("Audiobooks").into(),
                crate::audiobooks::subtitle(&self.lib),
            ),
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
            album_id: self
                .lib
                .albums_flat()
                .get(self.album_view)
                .map(|a| a.album_id)
                .unwrap_or(-1),
            expanded_album_id: self
                .album_expanded
                .and_then(|e| self.lib.albums_flat().get(e).map(|a| a.album_id))
                .unwrap_or(-1),
            artist_name: self
                .artist_name_at(self.artist_view)
                .unwrap_or("")
                .to_string(),
            playlist_id: self
                .lib
                .playlists
                .get(self.playlist_view)
                .map(|p| p.id)
                .unwrap_or(-1),
        }
    }

    /// Row of the album with `album_id` in the CURRENT library, if it is still there and the id
    /// actually identifies it.
    ///
    /// An id is only trusted when it is positive (0 is this codebase's "unset") and matches exactly
    /// ONE row. A duplicated or absent id resolves to `None` and the caller falls back to the
    /// stored index — picking the first of several matches would be the same class of mistake this
    /// whole change exists to fix, just with a different wrong answer.
    fn album_row_by_id(&self, album_id: i64) -> Option<usize> {
        if album_id <= 0 {
            return None;
        }
        let flat = self.lib.albums_flat();
        let mut hit = None;
        for (i, a) in flat.iter().enumerate() {
            if a.album_id == album_id {
                if hit.is_some() {
                    return None; // ambiguous — the id does not identify a row here
                }
                hit = Some(i);
            }
        }
        hit
    }

    /// Restore a pinned place. Two things this deliberately does NOT do any more: it no longer
    /// calls `go()` (which replaced the whole route stack, stranding the user with a dead Back
    /// button), and it no longer restores only the screen — the list position, sort and expansion
    /// come back too. Back from a restored pin returns to Now Playing.
    fn restore_pin(&mut self, p: &ShelfPin) {
        self.lib_tab = p.lib_tab;
        self.lib_sort = p.lib_sort.min(library::SORTS.len().saturating_sub(1));
        self.album_sort = p
            .album_sort
            .min(library::ALBUM_SORTS.len().saturating_sub(1));
        // Resolve by identity where we have one; the stored index is only the fallback for a pin
        // written before identities existed, or one whose album/artist/playlist has been deleted.
        self.album_expanded = self
            .album_row_by_id(p.expanded_album_id)
            .or(p.album_expanded);
        self.album_view = self.album_row_by_id(p.album_id).unwrap_or(p.album_view);
        self.artist_view = (!p.artist_name.is_empty())
            .then(|| {
                self.lib
                    .artists
                    .iter()
                    .position(|a| a.name == p.artist_name)
            })
            .flatten()
            .unwrap_or(p.artist_view);
        self.playlist_view = (p.playlist_id > 0)
            .then(|| {
                self.lib
                    .playlists
                    .iter()
                    .position(|pl| pl.id == p.playlist_id)
            })
            .flatten()
            .unwrap_or(p.playlist_view);
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
                        Tab::Albums,
                        &self.lib,
                        rank,
                        self.album_sort,
                        self.album_expanded,
                    );
                    self.lib_scroll_px = top.clamp(0, self.lib_max_scroll());
                }
            }
        }
    }

    /// Handle a tap while the Shelf overlay is open (geometry comes from `shelf::hit`).
    fn shelf_tap(&mut self, x: i32, y: i32) -> Vec<Action> {
        use crate::shelf::ShelfHit;
        let filled: [bool; crate::shelf::SLOTS] = std::array::from_fn(|i| self.pins[i].is_some());
        match crate::shelf::hit(x, y, filled) {
            ShelfHit::Close => self.shelf_open = false,
            ShelfHit::PinTo(i) => {
                if !pinnable(self.current()) {
                    self.notify(crate::i18n::text("Nothing to pin here"));
                } else {
                    let replaced = self.pins[i].is_some();
                    self.pins[i] = Some(self.capture_pin());
                    let name = self.pins[i]
                        .as_ref()
                        .map(|p| p.title.clone())
                        .unwrap_or_default();
                    // Say what happened AND which slot — the old silent "first empty, else
                    // clobber slot 0" left the user guessing.
                    self.notify_format(
                        if replaced {
                            "Slot {} replaced — {}"
                        } else {
                            "Pinned to slot {} — {}"
                        },
                        vec![("0", (i + 1).to_string()), ("1", name)],
                    );
                }
            }
            ShelfHit::Go(i) => {
                self.shelf_open = false;
                if let Some(p) = self.pins.get(i).and_then(|p| p.clone()) {
                    self.restore_pin(&p);
                    // The station list is the shell's to read (#55, plaquette: radio could not be
                    // pinned), as it is when the Wi-Fi page opens it.
                    if p.screen == Screen::NetRadio {
                        return vec![Action::RadioOpen];
                    }
                }
            }
            ShelfHit::Clear(i) => {
                if let Some(slot) = self.pins.get_mut(i) {
                    if slot.take().is_some() {
                        self.notify_format("Slot {} cleared", vec![("0", (i + 1).to_string())]);
                    }
                }
            }
            ShelfHit::None => {}
        }
        vec![]
    }

    /// Pop a transient toast (shelf feedback, queue confirmations).
    ///
    /// `pub` so the shell can reach it too: a low battery is something the user must be told, and
    /// the toast is the notification surface this UI already has. Inventing a second one for a
    /// message that appears twice in the life of a charge would be the wrong trade.
    pub fn notify(&mut self, msg: &str) {
        self.toast = msg.to_string();
        self.toast_raw = false;
        self.toast_template = None;
        self.toast_frames = TOAST_FRAMES;
    }

    /// Show an external message verbatim; it is not a UI catalog key.
    pub fn notify_raw(&mut self, msg: &str) {
        self.toast = msg.to_string();
        self.toast_raw = true;
        self.toast_template = None;
        self.toast_frames = TOAST_FRAMES;
    }

    /// Cache a dynamic notice by its exact catalog key and parameter names. Formatting at render
    /// time keeps the notice responsive to a language change after the action has happened.
    pub fn notify_format(&mut self, key: &'static str, args: Vec<(&'static str, String)>) {
        let owned: Vec<(String, String)> = args
            .into_iter()
            .map(|(name, value)| (name.to_string(), value))
            .collect();
        let english = crate::i18n::enter(crate::i18n::Language::English);
        let borrowed: Vec<(&str, &str)> = owned
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect();
        self.toast = crate::i18n::format_template(key, &borrowed);
        self.toast_raw = false;
        drop(english);
        self.toast_template = Some(ToastTemplate { key, args: owned });
        self.toast_frames = TOAST_FRAMES;
    }

    fn toast_for_render(&self) -> String {
        match &self.toast_template {
            Some(template) => {
                let args: Vec<(&str, &str)> = template
                    .args
                    .iter()
                    .map(|(name, value)| (name.as_str(), value.as_str()))
                    .collect();
                crate::i18n::format_template(template.key, &args)
            }
            None if self.toast_raw => self.toast.clone(),
            None => crate::i18n::text(&self.toast).to_string(),
        }
    }

    // ── Shelf pin persistence ───────────────────────────────────────────────────────────────
    // Pins were session-scoped, so every reboot wiped the user's bookmarks — the one thing a
    // "pin this place" feature must not do. They serialise to one line per slot; the shell
    // stores them in cinder_settings.conf alongside the rest.

    /// Serialise slot `i` as a `|`-separated record. Empty string = the slot is empty. `|` and
    /// newlines are stripped from the labels so a track title can't corrupt the config.
    pub fn shelf_pin_encode(&self, i: usize) -> String {
        let Some(p) = self.pins.get(i).and_then(|p| p.as_ref()) else {
            return String::new();
        };
        encode_pin(p)
    }

    /// Restore slot `i` from `shelf_pin_encode` output. A malformed or short record CLEARS the
    /// slot rather than failing — a hand-edited config must never keep the player from booting.
    pub fn shelf_pin_decode(&mut self, i: usize, s: &str) {
        if i >= self.pins.len() {
            return;
        }
        self.pins[i] = decode_pin(s);
    }

    // ── Home screen ─────────────────────────────────────────────────────────────────────────
    // What the player opens on (handoff 5a: "Start on", chosen on the Menu). Library is the
    // default — the handoff's answer to "what should the device show when it wakes". The shell
    // calls `apply_home` once, after the settings file is read.

    /// The home screen as its settings-file word.
    pub fn home_screen(&self) -> &'static str {
        HOME_WORDS[(self.home_screen as usize).min(HOME_WORDS.len() - 1)]
    }
    /// Set the home screen from its settings-file word. Anything unknown is the default, Library.
    pub fn set_home_screen(&mut self, v: &str) {
        self.home_screen = HOME_WORDS.iter().position(|w| *w == v.trim()).unwrap_or(0) as u8;
    }

    /// For "Last screen": the place to reopen, encoded like a Shelf pin. The topmost PLACE on the
    /// route (the same whitelist the Shelf uses), so opening Settings ▸ Display or a modal never
    /// replaces it. Scroll offsets are left out on purpose: this string is part of the settings
    /// file, and one that changed on every scrolled pixel would rewrite the file all the time.
    /// Empty unless Last screen is the home, so nobody else pays for the extra line.
    pub fn home_last_encode(&self) -> String {
        if self.home_screen != HOME_LAST {
            return String::new();
        }
        let Some(&screen) = self.stack.iter().rev().find(|s| pinnable(**s)) else {
            return String::new();
        };
        let mut p = self.capture_pin();
        p.screen = screen;
        p.lib_scroll_px = 0;
        p.album_scroll_px = 0;
        encode_pin(&p)
    }
    /// Read the place `home_last_encode` wrote. A malformed record is simply no place.
    pub fn set_home_last(&mut self, v: &str) {
        self.home_last = decode_pin(v);
    }

    /// Open on the home screen. Called once by the shell after the settings file is read. Does
    /// nothing unless the player is sitting on a fresh Now Playing — the intro, the lock screen or
    /// anything the user already opened wins.
    pub fn apply_home(&mut self) {
        if self.stack != [Screen::NowPlaying] || self.locked {
            return;
        }
        match self.home_screen {
            HOME_NOW_PLAYING => {}
            HOME_MENU => self.stack = vec![Screen::NowPlaying, Screen::Menu],
            HOME_LAST if self.home_last.is_some() => {
                if self.lib.is_empty() {
                    // The place may name an album: wait for the library it is an index into.
                    self.home_pending = true;
                } else if let Some(p) = self.home_last.clone() {
                    self.restore_pin(&p);
                }
            }
            // Library, and Last screen with nothing saved yet.
            _ => self.stack = vec![Screen::NowPlaying, Screen::Library],
        }
    }

    // ── Playback persistence ────────────────────────────────────────────────────────────────
    // A reboot used to throw the whole sequence away: play an album, power the player off, come
    // back to an empty Up Next with no idea where you were. Sony resumes; so does this.
    //
    // OBJECT IDS ONLY, never rows. The context can be the entire library after a "Shuffle all"
    // (3.6k tracks), and writing the titles/artists back out would be ~250 KB of duplicated
    // strings on a flash volume, to answer a question 8 bytes a track already answers. The shell
    // resolves the ids against the library it has just opened, so a file deleted between boots
    // simply drops out of the restored sequence instead of resurrecting a dead path.

    /// Serialise the play sequence and the un-shuffle order as id lists.
    pub fn playback_encode(&self) -> String {
        use std::fmt::Write as _;
        // Written once a second to be compared against the last body, so it reserves rather than
        // growing: the whole-library context is 3.6k ids and re-growing that string every tick is
        // the one avoidable cost on this path.
        let cap = (self.context.len() * 2 + self.history.len()) * 7 + 32;
        let mut s = String::with_capacity(cap);
        let push_ids = |s: &mut String, it: &mut dyn Iterator<Item = i64>| {
            for (i, id) in it.enumerate() {
                if i > 0 {
                    s.push(',');
                }
                let _ = write!(s, "{id}");
            }
        };
        s.push_str("ctx=");
        push_ids(&mut s, &mut self.context.iter().map(|r| r.object_id));
        // No `q=` or `pick=` lines any more: the queue is part of the sequence. `playback_restore`
        // still reads them, so a file written before the merge comes back with its picks in place.
        let _ = write!(s, "\nidx={}\n", self.context_idx);
        if let Some(pre) = &self.pre_shuffle {
            s.push_str("pre=");
            push_ids(&mut s, &mut pre.iter().copied());
            s.push('\n');
        }
        // PREVIOUSLY PLAYED. Bounded by `HISTORY_MAX` at the point of entry, so this is at most a
        // hundred ids however long the device has been running — see `App::history`. Written last
        // and read by key, so an older shell simply ignores the line.
        if !self.history.is_empty() {
            s.push_str("hist=");
            push_ids(&mut s, &mut self.history.iter().map(|r| r.object_id));
            s.push('\n');
        }
        // A user playlist's glue (`glue_pairs`), flattened as above,below pairs and sorted so the
        // body only changes when the glue does.
        if !self.glue_pairs.is_empty() {
            let mut pairs: Vec<_> = self.glue_pairs.iter().copied().collect();
            pairs.sort_unstable();
            s.push_str("glue=");
            push_ids(&mut s, &mut pairs.into_iter().flat_map(|(a, b)| [a, b]));
            s.push('\n');
        }
        s
    }

    /// Give MIX / the shuffle toggle a per-session starting seed. The shell calls this once at
    /// start-up with something clock-derived; cinder-ui has no clock of its own and must not grow
    /// one, because every one of its 300-odd host tests depends on this crate being deterministic.
    /// Zero is ignored — xorshift is stuck there.
    pub fn seed_shuffle(&mut self, seed: u64) {
        if seed != 0 {
            self.rng = crate::shuffle::Rng::new(seed);
        }
    }

    /// Record the order this context had BEFORE the shell shuffled it, so shuffle-off can put it
    /// back. Call AFTER `set_play_context`, which clears `pre_shuffle` by design.
    ///
    /// Without this, shuffle was still a one-way door on the commonest path of all. `queue_shuffle`
    /// records the pre-shuffle order, but that only runs when the toggle is pressed mid-playback.
    /// Turn shuffle ON first and then tap an album and the shell shuffles the rows before handing
    /// them over (`apply_shuffle`), so the context arrives already permuted with nothing saved —
    /// and shuffle-off then found no order to restore and silently did nothing. The un-shuffled
    /// order exists only out in the shell, so it has to be handed in.
    ///
    /// `ids` describes the sequence the shell just started. When "keep Up Next" appended the old
    /// list after it, the context is LONGER than that, and the kept rows are recorded in the order
    /// they already have. Ignored if it is longer than the context, which means it describes
    /// something else.
    pub fn note_pre_shuffle(&mut self, mut ids: Vec<i64>) {
        let n = ids.len();
        if n > self.context.len() {
            return;
        }
        // It must be an ORDER OF those rows — the same ids, each as often — or it describes some
        // other list, and unshuffling by it would scramble this one.
        let mut have: Vec<i64> = self.context[..n].iter().map(|t| t.object_id).collect();
        let mut want = ids.clone();
        have.sort_unstable();
        want.sort_unstable();
        if have != want {
            return;
        }
        ids.extend(self.context[n..].iter().map(|t| t.object_id));
        self.pre_shuffle = Some(ids);
    }

    /// Put a decoded context back. Rows come from the shell, which resolves the persisted ids
    /// against the library. Deliberately NOT `set_play_context`: that one is the "the user just
    /// started something" path and drops the un-shuffle order and the hand-added flag — both of
    /// which a restore must preserve.
    ///
    /// `queue` and `pick` are only non-empty for a file written before the queue merged into the
    /// sequence; see the splice below.
    pub fn playback_restore(
        &mut self,
        ctx: Vec<SongRow>,
        idx: usize,
        queue: Vec<SongRow>,
        pre: Option<Vec<i64>>,
        pick: Option<SongRow>,
    ) {
        let mut ctx = ctx;
        let mut idx = idx.min(ctx.len().saturating_sub(1));
        // A FILE WRITTEN BEFORE THE MERGE (2026-09-23) keeps the user's picks apart: the one that
        // was playing (`pick`, in neither list) and the ones still queued (`queue`), both of which
        // played before the rest of the context. Put them where they would have played — straight
        // after the context row they interrupted — so the first boot on the new build resumes on
        // the same song with the same things coming next.
        let spliced = pick.is_some() || !queue.is_empty();
        self.added_ids = pick.iter().chain(&queue).map(|r| r.object_id).collect();
        if spliced {
            let at = if ctx.is_empty() { 0 } else { idx + 1 };
            let had_pick = pick.is_some();
            ctx.splice(at..at, pick.into_iter().chain(queue));
            // A playing pick is what was audible, so it becomes the current row.
            if had_pick {
                idx = at;
            }
        }
        self.context_idx = idx.min(ctx.len().saturating_sub(1));
        self.context = ctx;
        self.hand_added = spliced;
        // Keep an un-shuffle order only while it still describes THIS sequence. A track deleted
        // between boots shortens one list and not the other, and a stale order would put tracks
        // back in the wrong places the first time shuffle went off.
        self.pre_shuffle = pre.filter(|p| p.len() == self.context.len());
        self.queue_scroll_px = 0;
        self.row_drag = None;
        self.queue_follow = true;
        self.up_next_cur = None;
        self.up_next_id = None;
    }

    /// Restore the play history from the resume file. Separate from `playback_restore` rather than
    /// a sixth positional argument to it: the history is independent of the context, the queue and
    /// the pick — it outlives all three — and five same-typed positional parameters is already the
    /// point where a call site starts getting them in the wrong order.
    ///
    /// Trimmed on the way in, so a hand-edited or older file cannot seed an unbounded list.
    pub fn history_restore(&mut self, rows: Vec<SongRow>) {
        self.history = rows;
        if self.history.len() > HISTORY_MAX {
            let excess = self.history.len() - HISTORY_MAX;
            self.history.drain(..excess);
        }
    }

    /// True when there is a sequence worth persisting (or, after a restore, worth handing to
    /// PlayerService on the first press of ▶).
    pub fn has_playback_state(&self) -> bool {
        !self.context.is_empty()
    }

    // ── Track information ───────────────────────────────────────────────────────────────────

    /// Replace the track-information rows. Called by the shell on every track change, so the
    /// screen is already correct the moment it is opened rather than fetching on entry.
    ///
    /// Resets the scroll: these are a DIFFERENT track's rows, and inheriting the last one's offset
    /// would open the screen part-way down a shorter list.
    pub fn set_track_info(&mut self, rows: Vec<(String, String)>) {
        self.track_info = rows;
        self.track_info_scroll_px = 0;
        self.track_info_h = 0;
        self.track_info_rows.clear();
    }

    pub fn track_info_rows(&self) -> &[(String, String)] {
        &self.track_info
    }

    /// How far the track-information list can scroll, from the height its last paint measured.
    fn track_info_max_scroll(&self) -> i32 {
        (self.track_info_h - (crate::track_info::BOTTOM - crate::track_info::TOP)).max(0)
    }

    /// The playing song's lyrics. Like `set_track_info`, a different song's lyrics open at the top
    /// and following again.
    pub fn set_lyrics(&mut self, lyrics: Option<crate::lyrics::Lyrics>) {
        self.lyrics = lyrics;
        self.lyrics_scroll_px = 0;
        self.lyrics_heights.clear();
        self.lyrics_pos_ms = 0;
        self.lyrics_follow = true;
    }

    /// Feed the playback position. True when it moved the highlighted line on the Lyrics screen —
    /// the only case worth a repaint, so the shell does not redraw a static page every second.
    pub fn set_lyrics_position(&mut self, pos_ms: u32) -> bool {
        let Some(l) = self.lyrics.as_ref() else {
            return false;
        };
        let moved = l.current(self.lyrics_pos_ms) != l.current(pos_ms);
        self.lyrics_pos_ms = pos_ms;
        moved && self.current() == Screen::Lyrics
    }

    /// The playing song has lyrics worth a screen: what draws Now Playing's Lyrics chip.
    pub fn has_lyrics(&self) -> bool {
        self.lyrics.as_ref().is_some_and(|l| !l.lines.is_empty())
    }

    /// Now Playing draws its Lyrics chip: the switch is on and the song has lyrics.
    pub fn lyrics_button(&self) -> bool {
        self.opt(Opt::NpLyrics) && self.has_lyrics()
    }

    fn open_lyrics(&mut self) {
        self.lyrics_follow = true;
        self.lyrics_scroll_px = 0;
        self.push(Screen::Lyrics);
    }

    fn lyrics_max_scroll(&self) -> i32 {
        crate::lyrics::max_scroll_px(&self.lyrics_heights)
    }

    /// The shell says whether the opt-in `search` component is installed.
    pub fn glue_pairs(&self) -> std::collections::HashSet<(i64, i64)> {
        self.glue_pairs.clone()
    }

    pub fn set_glue_pairs(&mut self, pairs: std::collections::HashSet<(i64, i64)>) {
        self.glue_pairs = pairs;
    }

    /// For `ids` in play order: is each glued to the one before it? (`shuffle::deal_glued`.)
    pub fn glue_mask(&self, ids: &[i64]) -> Vec<bool> {
        if self.glue_pairs.is_empty() {
            return Vec::new();
        }
        (0..ids.len())
            .map(|i| i > 0 && self.glue_pairs.contains(&(ids[i - 1], ids[i])))
            .collect()
    }

    pub fn set_hold_play_next(&mut self, on: bool) {
        self.hold_play_next = on;
    }

    pub fn set_search_enabled(&mut self, on: bool) {
        self.search_enabled = on;
    }

    /// Open library search with the keyboard already up: on an empty search page the only thing to
    /// do is type, so it should not take a second tap to start. The last query is kept, and
    /// re-run against the library as it is now.
    fn open_search(&mut self) {
        self.search_rebuild();
        self.fling_v = 0.0;
        self.push(Screen::Search);
        self.open_keyboard(KbPurpose::LibrarySearch);
    }

    /// Re-run the query. An empty one finds nothing rather than everything: this list is answers,
    /// and 3,400 unasked-for rows would read as a search that matched every song.
    fn search_rebuild(&mut self) {
        use crate::search::Hit;
        let q = self.search_query.trim().to_lowercase();
        let mut order = Vec::new();
        if !q.is_empty() {
            let has = |s: &str| s.to_lowercase().contains(&q);
            order.extend(
                (0..self.lib.artists.len())
                    .filter(|&i| has(&self.lib.artists[i].name))
                    .map(Hit::Artist),
            );
            order.extend(
                self.lib
                    .albums_flat()
                    .iter()
                    .enumerate()
                    .filter(|(_, a)| has(&a.name))
                    .map(|(i, _)| Hit::Album(i)),
            );
            order.extend(
                (0..self.lib.books.len())
                    .filter(|&i| has(&self.lib.books[i].title) || has(&self.lib.books[i].author))
                    .map(Hit::Book),
            );
            order.extend(self.songs_matching(&q).into_iter().map(Hit::Song));
        }
        self.search_order = order;
        self.search_scroll_px = 0;
    }

    /// The rows the Search screen draws, in result order.
    fn search_rows(&self) -> Vec<crate::search::HitRow> {
        use crate::search::{Hit, HitRow};
        let flat = self.lib.albums_flat();
        self.search_order
            .iter()
            .filter_map(|h| match *h {
                Hit::Artist(i) => self.lib.artists.get(i).map(|a| HitRow {
                    title: a.name.clone(),
                    sub: crate::i18n::text("Artist").to_string(),
                    opens: true,
                }),
                Hit::Album(i) => flat.get(i).map(|a| HitRow {
                    title: a.name.clone(),
                    sub: crate::ui_format!("Album · {}", a.artist),
                    opens: true,
                }),
                Hit::Book(i) => self.lib.books.get(i).map(|b| HitRow {
                    title: b.title.clone(),
                    sub: if b.author.is_empty() {
                        crate::i18n::text("Audiobook").to_string()
                    } else {
                        crate::ui_format!("Audiobook · {}", b.author)
                    },
                    opens: false,
                }),
                Hit::Song(i) => self.lib.songs.get(i).map(|s| HitRow {
                    title: s.title.clone(),
                    sub: s.artist.clone(),
                    opens: false,
                }),
            })
            .collect()
    }

    /// The search result at `y`, if any.
    fn search_hit_at(&self, y: i32) -> Option<crate::search::Hit> {
        if crate::playlist_pick::hit_search(y) {
            return None;
        }
        let r =
            crate::playlist_pick::hit_track_row(self.search_order.len(), self.search_scroll_px, y)?;
        self.search_order.get(r).copied()
    }

    /// The track row under `y` on the three plain track lists that have no page of their own —
    /// Search results, a folder's files, a SensMe channel's tracks. None on a folder or channel
    /// row, the search band, or empty space.
    fn list_song_at(&self, y: i32) -> Option<SongRow> {
        match self.current() {
            // Only song rows: an artist, album or book row has nothing to queue on its own.
            Screen::Search => match self.search_hit_at(y)? {
                crate::search::Hit::Song(i) => self.lib.songs.get(i).cloned(),
                _ => None,
            },
            Screen::Folders => {
                match crate::folders::row_at(&self.lib, self.folder_cur(), y, self.folder_scroll_px)
                {
                    Some(crate::folders::Row::Track(i)) => self
                        .folder_cur()
                        .and_then(|d| self.lib.folders.get(d))
                        .and_then(|d| d.tracks.get(i))
                        .cloned(),
                    _ => None,
                }
            }
            Screen::SensMe => match crate::sensme::row_at(
                &self.lib,
                self.sensme_channel,
                y,
                self.sensme_scroll_px,
            ) {
                Some(crate::sensme::Row::Track(i)) => self
                    .sensme_channel
                    .and_then(|c| self.lib.channels.get(c))
                    .and_then(|ch| ch.tracks.get(i))
                    .and_then(|&si| self.lib.songs.get(si as usize))
                    .cloned(),
                _ => None,
            },
            _ => None,
        }
    }

    fn search_max_scroll(&self) -> i32 {
        crate::playlist_pick::tracks_max_scroll(self.search_order.len())
    }

    /// Search: the band edits the query; an artist or album row opens its page, a book plays from
    /// its place (as on the Audiobooks screen), a song row plays that song.
    fn tap_search(&mut self, y: i32) -> Vec<Action> {
        use crate::search::Hit;
        if crate::playlist_pick::hit_search(y) {
            self.open_keyboard(KbPurpose::LibrarySearch);
            return vec![];
        }
        match self.search_hit_at(y) {
            Some(Hit::Artist(i)) => {
                self.open_artist(i);
                vec![]
            }
            Some(Hit::Album(i)) => {
                self.album_view = i;
                self.album_track_idx = 0;
                self.album_scroll_px = 0;
                self.fling_v = 0.0;
                self.push(Screen::Album);
                vec![]
            }
            Some(Hit::Book(i)) => self.start_play_action(Action::PlayBook(i)),
            Some(Hit::Song(i)) => match self.lib.songs.get(i).map(|s| s.object_id) {
                Some(id) => self.start_play(id),
                None => vec![],
            },
            None => vec![],
        }
    }

    /// True when the Now Playing screen is showing (so the shell only animates the visualiser
    /// there — animating elsewhere would waste battery under dirty-flag rendering).
    pub fn is_now_playing(&self) -> bool {
        self.current() == Screen::NowPlaying
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
        self.push_screen(s);
        self.note_browse();
    }

    /// Remember whether Library or Audiobooks is the one you are on; other screens leave it be.
    fn note_browse(&mut self) {
        match self.current() {
            Screen::Books => self.browse_books = true,
            Screen::Library => self.browse_books = false,
            _ => {}
        }
    }

    /// The Now Playing Library button: the Audiobooks list when that is where you last browsed.
    fn open_browse(&mut self) {
        if self.browse_books && !self.lib.books.is_empty() {
            self.open_books();
        } else {
            self.push(Screen::Library);
        }
    }

    fn push_screen(&mut self, s: Screen) {
        if self.current() != s {
            self.boot_stock_armed = false; // never leave a restart armed across a screen change
                                           // Arriving at Up Next always shows you the current track. Scrolling hands the list to
                                           // the user for as long as they stay on it; leaving and coming back is the reset, which
                                           // means there is no state to explain and no "jump to current" button to find.
            if s == Screen::UpNext {
                self.queue_follow = true;
                self.up_next_cur = None; // forces the next render to treat this as a track change
            }
            // Arriving at Settings or Display re-reads the palette folder, so a palette copied over
            // USB is there to pick without a reboot. The shell does the reading; this only asks.
            // Settings always opens on its home page.
            if s == Screen::Settings {
                self.settings_page = None;
                self.settings_scroll_px = 0;
            }
            if matches!(s, Screen::Settings | Screen::Display | Screen::Palette) {
                self.palettes_stale = true;
            }
            // Big view's scrolling pages open at the top.
            match s {
                Screen::Display => self.display_scroll_px = 0,
                Screen::Menu => self.menu_scroll_px = 0,
                Screen::BtCodec => self.bt_codec_scroll_px = 0,
                _ => {}
            }
            self.stack.push(s);
        }
    }
    fn pop(&mut self) {
        self.pop_screen();
        self.note_browse();
    }

    fn pop_screen(&mut self) {
        // Back inside the folder tree means UP ONE LEVEL, not "leave the library". Only at the
        // top of the tree does it pop the screen — which is the whole reason folder browse is its
        // own screen rather than a fifth Library tab.
        // Back inside a SensMe channel means "back to the channel list", not "leave SensMe" —
        // the same two-level rule the folder tree has below.
        if self.current() == Screen::SensMe && self.sensme_channel.is_some() {
            self.sensme_channel = None;
            self.sensme_scroll_px = self.sensme_scroll_saved;
            self.fling_v = 0.0;
            return;
        }
        // Media servers: up one level (a folder, then the server's top, then the server list).
        if self.current() == Screen::MediaServers && self.media_levels.len() > 1 {
            self.media_levels.pop();
            self.media_busy = false;
            self.fling_v = 0.0;
            return;
        }
        // Back while selecting folders cancels the selection, and nothing else.
        if self.current() == Screen::Folders && !self.folder_sel.is_empty() {
            self.folder_sel.clear();
            return;
        }
        if self.current() == Screen::Folders && !self.folder_stack.is_empty() {
            // …EXCEPT BACK OUT OF THE LEVEL `open_folders` SKIPPED ON THE WAY IN. With one storage
            // volume the root list is a single row you would always have to tap through, so
            // entering the tree descends past it — and coming back up landed on it anyway. Back
            // from the top of the tree therefore showed a one-row screen the design says never to
            // show, and needed a SECOND Back to leave. Skipped in both directions now.
            // Found by the 2026-09-06 UI audit, by the rule-test for the back chevron.
            let leaving_the_tree = self.folder_stack.len() == 1;
            if !(leaving_the_tree && self.lib.folder_roots.len() == 1) {
                self.folder_stack.pop();
                // Put the folder you came out of back under your finger. Without this, walking
                // back up a hundred-row root drops you at the top every time.
                self.folder_scroll_px = self.folder_scroll_saved.pop().unwrap_or(0);
                self.fling_v = 0.0;
                return;
            }
        }
        if self.stack.len() > 1 {
            self.boot_stock_armed = false;
            // Leaving the playlist editor any way but DONE leaves without saving — DONE clears the
            // state itself before it pops. Say so when there was something to lose.
            if self.current() == Screen::PlaylistEdit {
                self.row_drag = None;
                if self.pl_edit.take().is_some_and(|e| e.changed()) {
                    self.notify(crate::i18n::text("Changes not saved"));
                }
            }
            if self.current() == Screen::ViewEdit {
                self.view_edit = None;
            }
            self.stack.pop();
        }
    }

    // ── Folder browse ───────────────────────────────────────────────────────────────────────

    /// The directory currently being shown; `None` = the list of storage roots.
    fn folder_cur(&self) -> Option<usize> {
        self.folder_stack.last().copied()
    }

    /// Open folder browse at the top. With a single storage volume the root list would be one row
    /// you always have to tap through, so that level is skipped — it only appears when a microSD
    /// is present and there is a genuine choice to make.
    pub fn open_folders(&mut self) {
        self.folder_sel.clear();
        self.folder_stack.clear();
        self.folder_scroll_saved.clear();
        self.folder_scroll_px = 0;
        self.fling_v = 0.0;
        if self.lib.folder_roots.len() == 1 {
            self.folder_stack.push(self.lib.folder_roots[0]);
        }
        self.push(Screen::Folders);
    }

    /// The Menu rows actually drawn, in order.
    ///
    /// SensMe is in `MENU` but only appears when its component is installed: a row that opens a
    /// screen explaining a feature you chose not to install is clutter, not discovery. Everything
    /// that indexes the Menu — the cursor, the tap, the render — goes through this, so a hidden row
    /// cannot leave the cursor pointing one place and the picture another.
    fn menu_visible(&self) -> Vec<&'static (Screen, &'static str, &'static str)> {
        MENU.iter()
            .filter(|m| m.0 != Screen::SensMe || self.sensme_enabled)
            .filter(|m| m.0 != Screen::Zappin || self.preview_enabled)
            .filter(|m| m.0 != Screen::Wifi || crate::wifi::installed())
            .collect()
    }

    /// How many Menu rows are drawn (see [`App::menu_visible`]).
    fn menu_len(&self) -> usize {
        self.menu_visible().len()
    }

    /// The shell says whether the opt-in `sensme` component is installed.
    pub fn sensme_follow(&self) -> bool {
        self.sensme_follow
    }

    pub fn set_sensme_follow(&mut self, on: bool) {
        self.sensme_follow = on;
    }

    pub fn set_sensme_enabled(&mut self, on: bool) {
        self.sensme_enabled = on;
        // The cursor is an index into the VISIBLE rows, and this changes how many there are.
        self.menu_idx = self.menu_idx.min(self.menu_len().saturating_sub(1));
    }

    /// The shell says whether the opt-in `preview` component is installed. Like SensMe, it gates
    /// the Menu row only: nothing else starts a preview pass.
    pub fn set_preview_enabled(&mut self, on: bool) {
        self.preview_enabled = on;
        self.menu_idx = self.menu_idx.min(self.menu_len().saturating_sub(1));
    }

    /// Mirror the shell-owned preview session for the Menu control labels/actions.
    pub fn set_zappin_active(&mut self, active: bool) {
        self.zappin_active = active;
    }

    /// Show that the finite preview pass ended naturally or was stopped from its Menu control.
    pub fn set_zappin_completed(&mut self, completed: bool) {
        self.zappin_completed = completed;
    }

    /// Mirror the confirmed candidate's source: SABI is an analyzed start point, not proof of a
    /// chorus; fixed and unknown-duration fallbacks are named separately.
    pub fn set_zappin_source(&mut self, source: Option<crate::zappin::StartSource>) {
        self.zappin_source = source;
        if source.is_none() {
            self.zappin_start_ms = None;
        }
    }

    pub fn set_zappin_start_ms(&mut self, start_ms: Option<u64>) {
        self.zappin_start_ms = start_ms;
    }

    /// Live playback timer from the shell. It advances only while the service is playing.
    pub fn set_zappin_progress(
        &mut self,
        elapsed_ms: u32,
        length_ms: u32,
        current: u32,
        total: u32,
    ) {
        self.zappin_elapsed_ms = elapsed_ms.min(length_ms);
        self.zappin_length_ms = length_ms;
        self.zappin_current = current;
        self.zappin_total = total;
    }

    /// Return to ordinary Now Playing only after the shell has confirmed full playback.
    pub fn complete_zappin_full_play(&mut self) {
        self.zappin_active = false;
        self.zappin_completed = false;
        self.go(Screen::NowPlaying);
    }

    fn zappin_menu_subtitle(&self) -> String {
        if self.zappin_active {
            String::from(crate::i18n::text("Preview active · tap to return"))
        } else {
            String::new()
        }
    }

    /// Open the channel list from the top.
    pub fn open_sensme(&mut self) {
        self.sensme_channel = None;
        self.sensme_scroll_px = 0;
        self.sensme_scroll_saved = 0;
        self.fling_v = 0.0;
        self.push(Screen::SensMe);
    }

    /// Open the Audiobooks list from the top.
    pub fn open_books(&mut self) {
        self.books_scroll_px = 0;
        self.fling_v = 0.0;
        self.push(Screen::Books);
    }

    /// The Books side of the switch on the Library: the Audiobooks list in the Library's place, so
    /// Back leaves for where the Library was opened from.
    fn library_to_books(&mut self) {
        self.books_scroll_px = 0;
        self.fling_v = 0.0;
        if let Some(top) = self.stack.last_mut() {
            *top = Screen::Books;
        }
        self.note_browse();
    }

    /// The Music side of the switch on the Audiobooks list: the Library in its place.
    fn books_to_music(&mut self) {
        if let Some(top) = self.stack.last_mut() {
            *top = Screen::Library;
        }
        self.note_browse();
    }

    /// Tap on the Audiobooks screen: a book plays from its place.
    fn tap_books(&mut self, y: i32) -> Vec<Action> {
        match crate::audiobooks::row_at(&self.lib, y, self.books_scroll_px) {
            Some(i) => self.start_play_action(Action::PlayBook(i)),
            None => vec![],
        }
    }

    /// Open one channel, remembering where the list was so Back can restore it.
    fn sensme_enter(&mut self, i: usize) {
        if i >= self.lib.channels.len() {
            return;
        }
        self.sensme_scroll_saved = self.sensme_scroll_px;
        self.sensme_channel = Some(i);
        self.sensme_scroll_px = 0;
        self.fling_v = 0.0;
    }

    /// Tap on the SensMe screen: a row, or the open channel's PLAY | SHUFFLE band.
    fn tap_sensme(&mut self, x: i32, y: i32) -> Vec<Action> {
        // The band belongs to the open channel only, and it is drawn ABOVE the list, so it is
        // asked first — `row_at` already refuses everything above `list_top`.
        if let Some(i) = self.sensme_channel {
            let chan = self.lib.channels.get(i).map(|c| c.id);
            if let Some(chan) = chan {
                if crate::sensme::hit_play_band(x, y) {
                    return self.sensme_play(chan, 0, false);
                }
                if crate::sensme::hit_shuffle_band(x, y) {
                    return self.sensme_play(chan, 0, true);
                }
            }
        }
        // The channel GRID (handoff 5d): a tile opens its channel; the foot's switch and button.
        if self.sensme_channel.is_none() {
            if self.lib.channels.is_empty() {
                return vec![]; // the explained empty screen: nothing is drawn to tap
            }
            if let Some(i) =
                crate::sensme::tile_at(self.lib.channels.len(), x, y, self.sensme_scroll_px)
            {
                self.sensme_enter(i);
                return vec![];
            }
            if crate::sensme::follow_hit(y) {
                self.sensme_follow = !self.sensme_follow;
                return vec![];
            }
            if crate::sensme::button_hit(x, y) {
                let foot = crate::sensme::Foot {
                    follow: self.sensme_follow,
                    hour: self.clock_hour,
                };
                return match crate::sensme::foot_channel(&self.lib, foot)
                    .and_then(|i| self.lib.channels.get(i))
                {
                    Some(ch) => {
                        let id = ch.id;
                        self.sensme_play(id, 0, false)
                    }
                    // Every analysed track, shuffled — Sony's fourteenth tile.
                    None => self.sensme_play(crate::sensme::ALL, 0, true),
                };
            }
            return vec![];
        }
        match crate::sensme::row_at(&self.lib, self.sensme_channel, y, self.sensme_scroll_px) {
            Some(crate::sensme::Row::Channel(i)) => {
                self.sensme_enter(i);
                vec![]
            }
            Some(crate::sensme::Row::Track(i)) => {
                let chan = self
                    .sensme_channel
                    .and_then(|c| self.lib.channels.get(c))
                    .map(|c| c.id);
                match chan {
                    Some(chan) => self.sensme_play(chan, i as u32, false),
                    None => vec![],
                }
            }
            None => vec![],
        }
    }

    /// Play a channel. The CHANNEL is the context — the same reason `PlayPlaylistAt` exists: an
    /// object id only knows its album, so playing a channel member through `PlayIndex` would play
    /// that track's album and leave the channel behind after one song.
    fn sensme_play(&mut self, chan: u8, from: u32, shuffle: bool) -> Vec<Action> {
        self.start_play_action(Action::PlaySensMe {
            chan,
            from,
            shuffle,
        })
    }

    /// Descend into `i`, remembering where we were so Back can restore it.
    fn folder_toggle(&mut self, i: usize) {
        match self.folder_sel.iter().position(|&s| s == i) {
            Some(at) => {
                self.folder_sel.remove(at);
            }
            None => self.folder_sel.push(i),
        }
    }

    fn folder_enter(&mut self, i: usize) {
        if i >= self.lib.folders.len() {
            return;
        }
        self.folder_sel.clear();
        self.folder_scroll_saved.push(self.folder_scroll_px);
        self.folder_stack.push(i);
        self.folder_scroll_px = 0;
        self.fling_v = 0.0;
    }

    /// Play the current directory's tracks, starting at `i`. The CONTEXT is the folder — which is
    /// the point of browsing this way: what follows should be the rest of the folder, in the order
    /// the files are in, not whatever the last album left behind.
    ///
    /// It used to emit `PlayIndex`, which resolves the tapped track's ALBUM — so the comment above
    /// was true about the design and false about the code.
    fn folder_play(&mut self, i: usize) -> Vec<Action> {
        let Some(ids) = self
            .folder_cur()
            .and_then(|d| self.lib.folders.get(d))
            .map(|d| d.tracks.iter().map(|t| t.object_id).collect::<Vec<i64>>())
        else {
            return vec![];
        };
        self.start_play_list(ids, i)
    }

    fn tap_folders(&mut self, x: i32, y: i32) -> Vec<Action> {
        // SELECTING (#56): ADD takes every ticked folder to the picker; a folder row ticks or
        // unticks; song rows do nothing until the selection is added or Back cancels it.
        if !self.folder_sel.is_empty() {
            if crate::chrome::header_action_hit(x, y) {
                let sel = std::mem::take(&mut self.folder_sel);
                self.open_playlist_pick_folders(&sel);
            } else if let Some(crate::folders::Row::Dir(i)) =
                crate::folders::row_at(&self.lib, self.folder_cur(), y, self.folder_scroll_px)
            {
                self.folder_toggle(i);
            }
            return vec![];
        }
        match crate::folders::row_at(&self.lib, self.folder_cur(), y, self.folder_scroll_px) {
            Some(crate::folders::Row::Dir(i)) => {
                self.folder_enter(i);
                vec![]
            }
            Some(crate::folders::Row::Track(i)) => self.folder_play(i),
            None => vec![],
        }
    }

    // Activate the focused Settings row (shared by the Select button and a tap).
    /// The Menu's live row subtitles. Extracted from `render` so the strings themselves are
    /// testable: `render` only produces pixels, so a mock value creeping back in (this table used
    /// to carry the prototype's invented "124 albums · 1,842 tracks" and "WH-1000XM5 · LDAC") could
    /// not otherwise be caught by a test. Every field below reports state this App actually holds.
    pub(crate) fn menu_subtitles(&self) -> MenuSubtitles {
        MenuSubtitles {
            library: if self.lib.is_empty() {
                String::from(crate::i18n::text("Empty"))
            } else {
                crate::ui_format!(
                    "{} albums · {} tracks",
                    self.lib.album_count(),
                    self.lib.songs.len()
                )
            },
            // How much analysis the library actually carries. "Not analysed" rather than "Empty":
            // an empty SensMe list is not a small library, it is an untagged one, and the screen
            // behind the row says what tags it.
            sensme: match (self.lib.channels.len(), self.lib.sensme_tracks) {
                (0, _) => String::from(crate::i18n::text("Not analysed")),
                (1, n) => crate::ui_format!("1 channel · {n} tracks", n = n),
                (c, n) => crate::ui_format!("{c} channels · {n} tracks", c = c, n = n),
            },
            // Counts the DIRECTORIES, which is the thing this row leads to — the track total is
            // already on the Library row above it and repeating it would say nothing new.
            folders: match self.lib.folders.len() {
                0 => String::from(crate::i18n::text("Empty")),
                1 => String::from(crate::i18n::text("1 folder")),
                n => crate::ui_format!("{n} folders", n = n),
            },
            bluetooth: self.bt_codec_name().to_string(),
            usb_dac: String::from(if self.usb_dac_on {
                crate::i18n::text("On")
            } else {
                crate::i18n::text("Off")
            }),
            // The sound, and which of the two levels is in force is not worth the width: the page
            // says it. "Off" when switched off, even though a sound stays chosen.
            // The network's name once on one; otherwise the switch's state, as the page says it.
            wifi: match self.wifi_status.state {
                crate::wifi::State::Connected => {
                    String::from_utf8_lossy(&self.wifi_status.ssid).into_owned()
                }
                crate::wifi::State::Off => String::from(crate::i18n::text("Off")),
                crate::wifi::State::Busy => String::from(crate::i18n::text("Working…")),
                crate::wifi::State::On | crate::wifi::State::Connecting => {
                    String::from(crate::i18n::text("On · not connected"))
                }
            },
            soundscape: if self.ambient_on {
                crate::i18n::text(crate::soundscape::label(self.ambient_sound)).to_string()
            } else {
                String::from(crate::i18n::text("Off"))
            },
            // Name the effects actually engaged; "Off" when the chain is clean.
            // What is actually shaping the sound, in the order the Sound screen's footer resolves
            // it: an override first, because it is what you would have to turn off. `snd_clear` is
            // ClearAudio+ — it was listed here as "Clear Phase", which is a different effect on
            // the Advanced screen.
            sound: if self.adv_source_direct {
                String::from(crate::i18n::text("Source Direct"))
            } else {
                let on: Vec<&str> = if self.snd_clear {
                    let mut v = vec!["ClearAudio+"];
                    if self.snd_norm {
                        v.push(crate::i18n::text("Normalizer"));
                    }
                    v
                } else {
                    [
                        ("DSEE HX", self.snd_dsee),
                        (crate::i18n::text("Vinyl"), self.snd_vinyl),
                        ("VPT", self.snd_vpt),
                        (crate::i18n::text("DC Phase"), self.snd_dc),
                        (crate::i18n::text("Normalizer"), self.snd_norm),
                    ]
                    .iter()
                    .filter(|(_, en)| *en)
                    .map(|(n, _)| *n)
                    .collect()
                };
                if on.is_empty() {
                    String::from(crate::i18n::text("Off"))
                } else {
                    on.join(" · ")
                }
            },
        }
    }

    /// Act on the focused Settings ▸ Display row (the Select button, or a tap that is not on a
    /// swatch, a chip or the slider — those are handled in `tap` from the finger's x).
    fn display_activate(&mut self) -> Vec<Action> {
        match self.display_sel {
            crate::display::ROW_PALETTE => {
                // The picker (handoff 5j). It used to cycle one palette per tap, which made the
                // fifth file four unwanted colour schemes away and reported a refused file only in
                // the log. The folder is re-read on the way in (see `palettes_stale`).
                self.palette_scroll_px = 0;
                self.push(Screen::Palette);
                vec![]
            }
            crate::display::ROW_ACCENT => {
                if self.palette_pins_accent() {
                    // Cycling here would change a colour nothing is drawn in. Say why instead.
                    self.notify(crate::i18n::text("This palette sets its own accent"));
                } else {
                    self.accent = self.accent.next();
                }
                vec![]
            }
            crate::display::ROW_NIGHT => self.set_night_mode((self.night_mode() + 1) % 3),
            // The schedule's rows take effect on the next clock tick (within a second): a changed
            // window is looked at afresh, so a schedule switched on at 22:00 brings Night on now.
            crate::display::ROW_NIGHT_FROM => {
                self.night_from =
                    crate::display::next_time(&crate::display::NIGHT_STARTS, self.night_from);
                self.night_in_window = None;
                vec![]
            }
            crate::display::ROW_NIGHT_UNTIL => {
                self.night_until =
                    crate::display::next_time(&crate::display::NIGHT_ENDS, self.night_until);
                self.night_in_window = None;
                vec![]
            }
            crate::display::ROW_VOLUME => {
                self.volume_hud = (self.volume_hud + 1) % crate::display::VOLUME_HUDS.len() as u8;
                self.show_volume_hud_sample();
                vec![]
            }
            // Select steps the text size one stop (a tap uses the finger's x instead).
            crate::display::ROW_SIZE => self.step_ui_scale(1),
            crate::display::ROW_STYLE => {
                self.style = crate::style::Style::from_index(
                    (self.style.index() + 1) % crate::style::Style::COUNT,
                );
                vec![]
            }
            _ => {
                self.viz_sel = 0;
                self.push(Screen::VizSet);
                vec![]
            }
        }
    }

    /// Show the volume readout for a moment, so choosing Full or Minimal shows what was chosen.
    fn show_volume_hud_sample(&mut self) {
        self.vol_overlay = crate::overlay::VOL_FRAMES;
    }

    /// Back from a Settings topic page: the home page, cursor on that topic's link, scrolled just
    /// far enough to show it. In Big view the home page is taller than the screen, and from the top
    /// the lower topics' links are off the bottom (seen on the player 11-03: back from Power showed
    /// Display to Library, with the cursor out of sight).
    fn settings_page_back(&mut self) {
        let g = self.settings_page.unwrap_or(0);
        self.open_settings_page(None);
        let link = crate::settings::group_row(g);
        self.settings_sel = link;
        let lang = self.settings_lang();
        let rows = crate::settings::page_rows(None, lang);
        if let Some(i) = rows.iter().position(|r| *r == link) {
            let bottom = crate::settings::LIST_TOP + (i as i32 + 1) * crate::settings::RH();
            let max = crate::settings::max_scroll_px(None, lang);
            self.settings_scroll_px = (bottom + 8 - crate::canvas::H as i32).clamp(0, max);
        }
    }

    /// Show a Settings page (`None` = home) from its top, cursor on its first row.
    fn open_settings_page(&mut self, page: Option<usize>) {
        self.settings_page = page;
        self.settings_scroll_px = 0;
        if let Some(r) = crate::settings::page_rows(page, self.settings_lang()).first() {
            self.settings_sel = *r;
        }
    }

    fn settings_activate(&mut self) -> Vec<Action> {
        // Touching any other row cancels a pending boot-to-stock confirmation, so the armed state
        // can never linger and turn a later, unrelated tap on that row into a restart.
        if self.settings_sel != crate::settings::ROW_BOOT_STOCK {
            self.boot_stock_armed = false;
        }
        // An opt-in switch row (`opt.rs`). Render-only: the shell's save path writes the file.
        if let Some(o) = Opt::of_row(self.settings_sel) {
            self.toggle_opt(o);
            return vec![];
        }
        match self.settings_sel {
            crate::settings::ROW_LANGUAGE => {
                self.language_scroll_px = 0;
                self.push(Screen::Language);
                vec![]
            }
            // A topic link on the home page: open its page, cursor on its first row.
            r if crate::settings::group_of_link(r).is_some() => {
                self.open_settings_page(crate::settings::group_of_link(r));
                vec![]
            }
            crate::settings::ROW_DISPLAY => {
                self.push(Screen::Display);
                vec![]
            }
            crate::settings::ROW_CLOCK => {
                // Seed from the last epoch the shell reported, so the editor opens on the current
                // time rather than on whatever was left from the previous visit.
                if self.clock_epoch > 0 {
                    self.clock_fields = crate::clockset::fields_from_epoch(self.clock_epoch);
                }
                self.clock_sel = 0;
                self.push(Screen::ClockSet);
                vec![]
            }
            crate::settings::ROW_WIFI => {
                self.push(Screen::Wifi);
                vec![]
            }
            crate::settings::ROW_ALARM => {
                self.alarm_sel = 0;
                self.push(Screen::Alarm);
                vec![]
            }
            crate::settings::ROW_VOLUME_LIMIT => {
                // Toggle only. There is deliberately no number to cycle: the cap is Sony's AVLS
                // threshold for whatever output is live (`adapt=1`), read by the shell at the
                // moment it clamps, so a value picked here would go stale the moment you plugged
                // something different in.
                self.toggle_volume_limit();
                // Render-only, like the accent row: the settings file is written by the shell's
                // own save path, and the shell notices the switch on its next housekeeping tick
                // and re-applies the volume through the clamp.
                vec![]
            }
            crate::settings::ROW_IGNORE_THE => {
                // Render-only, like Volume limit: the shell's save path writes the file.
                self.set_ignore_the(!self.ignore_the);
                vec![]
            }
            crate::settings::ROW_BIG => {
                let on = !crate::scale::large();
                self.set_big_view(on);
                vec![Action::UiScaleChanged]
            }
            crate::settings::ROW_BIG_BAR => {
                crate::scale::set_big_bar(!crate::scale::big_bar_set());
                vec![Action::UiScaleChanged]
            }
            crate::settings::ROW_BIG_ICONS => {
                crate::scale::set_big_icons(!crate::scale::big_icons_set());
                vec![Action::UiScaleChanged]
            }
            crate::settings::ROW_SWIPE_UP => {
                // Render-only, like the Folders tab: the shell's save path writes the file.
                self.np_swipe_up = self.np_swipe_up.next();
                vec![]
            }
            crate::settings::ROW_SWIPE_DOWN => {
                self.np_swipe_down = self.np_swipe_down.next();
                vec![]
            }
            crate::settings::ROW_NP_BUTTON => {
                self.np_button = self.np_button.next();
                vec![]
            }
            crate::settings::ROW_SHUFFLE => {
                // Render-only, like Ignore "The": the shell's save path writes the file. It
                // changes what the NEXT press of the shuffle button deals, not the list playing.
                self.shuffle_by = self.shuffle_by.next();
                vec![]
            }
            crate::settings::ROW_SLEEP => {
                // One list for this row and the pull-down panel's chips.
                self.set_sleep_choice((self.sleep_idx + 1) % crate::quick::SLEEP_PRESETS.len())
            }
            crate::settings::ROW_DATABASE => {
                // The row had no arm at all, which is why it was drawn without a chevron: a chevron
                // is this screen's promise that tapping does something, and a rebuild that never ran
                // would have been a lie. It runs now.
                self.rescanning = true;
                vec![Action::RescanLibrary]
            }
            crate::settings::ROW_STORAGE => {
                self.storage_scroll_px = 0;
                self.push(Screen::Storage);
                vec![]
            }
            crate::settings::ROW_BATTERY => {
                // Was a toggle in place. It is a chevron into the Device screen now — the care
                // switch lives there, next to the readings that explain why you would want it.
                self.push(Screen::Device);
                vec![]
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
            crate::settings::ROW_RESET => {
                self.confirm = Some(crate::confirm::Ask::ResetSettings);
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
            // No Action: the shell polls `cinder_get_bt_idle_off` from its 1 Hz housekeeping.
            crate::settings::ROW_BT_IDLE_OFF => {
                self.bt_idle_off = !self.bt_idle_off;
                vec![]
            }
            crate::settings::ROW_AUTO_OFF => {
                self.auto_off_idx = (self.auto_off_idx + 1) % AUTO_OFF_PRESETS.len();
                self.auto_off_min = AUTO_OFF_PRESETS[self.auto_off_idx];
                // No Action: the shell polls cinder_get_auto_off_min() from its 1 Hz housekeeping,
                // where the idle timer it feeds already lives. A one-shot action would only tell it
                // something it is about to read anyway.
                vec![]
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
                // 1..5 and wrap. The 6th stop used to be 0 = BACKLIGHT OFF, and it is PARKED
                // rather than deleted — see the note on `brightness` below. The shell's level-0
                // handling and its Hold/Power escape are intact and unused, ready for the real
                // feature.
                self.brightness = match self.brightness {
                    n if n >= 5 => 1,
                    0 => 1,
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

    /// Why the 10-band EQ is not being heard, or None when it is. Outer bypass first, the order
    /// the Sound screen's footer uses, so the two screens give the same answer.
    fn eq_off_reason(&self) -> Option<&'static str> {
        if self.adv_source_direct {
            Some(crate::i18n::text("Off: Source Direct is on"))
        } else if self.snd_clear {
            Some(crate::i18n::text("Off: ClearAudio+ is on"))
        } else if self.adv_tone {
            Some(crate::i18n::text("Off: Tone Control is on"))
        } else {
            None
        }
    }

    // Toggle the focused Sound effect row (shared by the Select button and a tap).
    fn sound_toggle_row(&mut self) -> Vec<Action> {
        match self.sound_sel {
            0 => self.snd_dsee = !self.snd_dsee,
            1 => self.snd_vinyl = !self.snd_vinyl,
            // VPT is one control with five states, not a toggle plus a hidden mode: there is no
            // d-pad on this device, so a tap is the ONLY way to reach the rooms. Cycles
            // Off -> Studio -> Club -> Concert Hall -> Matrix -> Off. `snd_vpt` and `snd_vpt_mode`
            // stay separate underneath because the device has separate SetVpt/SetVptMode calls.
            2 if self.w1_chain => return vec![],
            2 => {
                if !self.snd_vpt {
                    self.snd_vpt = true;
                    self.snd_vpt_mode = 0;
                } else if self.snd_vpt_mode + 1 < VPT_MODES.len() {
                    self.snd_vpt_mode += 1;
                } else {
                    self.snd_vpt = false;
                    self.snd_vpt_mode = 0;
                }
            }
            // Same one-control-many-states shape as VPT above, and for the same reason: no d-pad.
            3 => {
                if !self.snd_dc {
                    self.snd_dc = true;
                    self.snd_dc_type = 0;
                } else if self.snd_dc_type + 1 < DC_PHASE_TYPES.len() {
                    self.snd_dc_type += 1;
                } else {
                    self.snd_dc = false;
                    self.snd_dc_type = 0;
                }
            }
            4 => self.snd_norm = !self.snd_norm,
            // Balance is a slider, not a toggle. Select on it means "back to centre" — the one
            // thing a button can usefully do to a continuous control, and the position a user is
            // most likely to want back.
            crate::sound::ROW_BALANCE => self.snd_balance = crate::sound::BALANCE_CENTRE,
            // Not a setting — a route. Returns no action: pushing a screen changes nothing about
            // the sound, and emitting SoundChanged here would re-apply the whole chain on a
            // navigation, which is how a "why did my audio blip when I opened a menu" bug starts.
            crate::sound::ROW_ADVANCED => {
                self.adv_sel = crate::advanced::ROW_VPT;
                self.adv_scroll_px = 0;
                self.push(Screen::Advanced);
                return vec![];
            }
            // Routes too, for the same reason: which profile each output uses, and the EQ editor.
            crate::sound::ROW_PROFILE => {
                self.presets_scroll_px = 0;
                self.push(Screen::Presets);
                return vec![];
            }
            crate::sound::ROW_EQ => {
                if self.eq_on_dac() {
                    self.dac_eq_sel = 0;
                    self.push(Screen::DacEq);
                } else {
                    self.push(Screen::Eq);
                }
                return vec![];
            }
            _ => {}
        }
        // A change to a row that is out of the path is saved but not heard — say so, or the tap
        // looks like it did nothing. Same wording as the row's own dimmed subtitle.
        let bypassed = match self.sound_sel {
            0..=3 if self.adv_source_direct || self.snd_clear => true,
            4 if self.adv_source_direct => true,
            _ => false,
        };
        if bypassed {
            self.notify(if self.adv_source_direct {
                crate::i18n::text("Saved — off in Direct mode")
            } else {
                crate::i18n::text("Saved — off in ClearAudio+ mode")
            });
        }
        vec![Action::SoundChanged]
    }

    /// Sound ▸ the Direct / Custom / ClearAudio+ control. The mode is the two flags the service
    /// already has (Source Direct, ClearAudio+), so nothing new is stored and an older settings
    /// file reads as the mode it always meant.
    fn set_sound_mode(&mut self, mode: usize) -> Vec<Action> {
        use crate::sound::{mode_of, MODE_CLEARAUDIO, MODE_DIRECT};
        if mode == mode_of(self.adv_source_direct, self.snd_clear) {
            return vec![];
        }
        self.adv_source_direct = mode == MODE_DIRECT;
        self.snd_clear = mode == MODE_CLEARAUDIO;
        vec![Action::SoundChanged]
    }

    /// Toggle/cycle the focused Advanced row (shared by the Select button and a tap).
    ///
    /// Every row here is a tap-cycle for the same reason the VPT room is: no d-pad, so a value a
    /// finger cannot reach is a value that does not exist.
    fn advanced_toggle_row(&mut self) -> Vec<Action> {
        use crate::advanced as adv;
        match self.adv_sel {
            // Clear Bass has DC Phase's place in this player's chain.
            adv::ROW_DC if self.clear_bass_available => return vec![],
            // The three effects that moved here from the Sound screen keep its state and its
            // toggles, bypass toast included.
            adv::ROW_VPT | adv::ROW_DC | adv::ROW_VINYL => {
                self.sound_sel = match self.adv_sel {
                    adv::ROW_VPT => crate::sound::ROW_VPT,
                    adv::ROW_DC => crate::sound::ROW_DC,
                    _ => crate::sound::ROW_VINYL,
                };
                return self.sound_toggle_row();
            }
            adv::ROW_CLEAR_PHASE | adv::ROW_DSEE_AI if self.w1_chain => return vec![],
            adv::ROW_CLEAR_PHASE => self.adv_clear_phase = !self.adv_clear_phase,
            adv::ROW_DSEE_AI => self.adv_dsee_ai = !self.adv_dsee_ai,
            adv::ROW_DSEE_CUSTOM => {
                if !self.adv_dsee_custom {
                    self.adv_dsee_custom = true;
                    self.adv_dsee_mode = 0;
                } else if self.adv_dsee_mode + 1 < adv::DSEE_MODES.len() {
                    self.adv_dsee_mode += 1;
                } else {
                    self.adv_dsee_custom = false;
                    self.adv_dsee_mode = 0;
                }
            }
            // Vinyl character has no "off" — the Vinyl Processor's own switch is on the Sound
            // screen, and this only says WHICH character that switch turns on. So it wraps
            // through the four and never lands on a state that means "none".
            adv::ROW_VINYL_TYPE => {
                self.adv_vinyl_type = (self.adv_vinyl_type + 1) % adv::VINYL_TYPES.len();
            }
            adv::ROW_TONE => self.adv_tone = !self.adv_tone,
            adv::ROW_CLEAR_BASS => {
                self.adv_clear_bass = (self.adv_clear_bass + 1) % adv::CLEAR_BASS.len();
            }
            adv::ROW_HP_AMP => self.adv_hp_linear = !self.adv_hp_linear,
            // Not Sony's chain: the FFI keeps /tmp/cinder_crossfeed in step with the switch, so
            // there is nothing to re-apply. With Mono on the switch does nothing (and says so).
            adv::ROW_CROSSFEED => {
                if !self.mono {
                    self.toggle_opt(Opt::Crossfeed);
                }
                return vec![];
            }
            // Off → Track → Album → Off. Also the FFI's to carry out (/tmp/cinder_rg).
            adv::ROW_REPLAYGAIN => {
                let (on, album) = (self.opt(Opt::ReplayGain), self.opt(Opt::RgAlbum));
                self.set_opt(Opt::ReplayGain, !(on && album));
                self.set_opt(Opt::RgAlbum, on && !album);
                return vec![];
            }
            // The shell's: it lists the profiles on the drive and plays the next one.
            adv::ROW_HP_EQ => return vec![Action::NextHpEq],
            // A ROUTE, not a setting — same rule as the Sound screen's "Advanced ›" row: pushing a
            // screen changes nothing about the sound, and emitting SoundChanged here would
            // re-apply the whole chain on a navigation.
            adv::ROW_TONE_BANDS => {
                self.tone_sel = 0;
                self.push(Screen::Tone);
                return vec![];
            }
            // A route too. The DAC EQ has its own action, so nothing here re-applies anything.
            adv::ROW_DAC_EQ => {
                self.dac_eq_sel = 0;
                self.push(Screen::DacEq);
                return vec![];
            }
            adv::ROW_VOL_CURVE => {
                self.vol_curve_sel = 0;
                self.push(Screen::VolCurve);
                return vec![];
            }
            _ => {}
        }
        vec![Action::SoundChanged]
    }

    /// Nudge the focused Tone Control band. `dir` is +1 up / -1 down; one step is 1.0 dB.
    ///
    /// The clamp is not cosmetic: past ±20 raw the sound service does not clamp to the edge, it
    /// ZEROES the band (measured — see `tone.rs`), so an unclamped nudge would read as a boost and
    /// be silence.
    fn tone_nudge(&mut self, dir: i32) -> Vec<Action> {
        let g = &mut self.tone_bands[self.tone_sel];
        *g = if dir > 0 {
            (*g + crate::tone::BAND_STEP).min(crate::tone::BAND_MAX)
        } else {
            (*g - crate::tone::BAND_STEP).max(-crate::tone::BAND_MAX)
        };
        vec![Action::SoundChanged]
    }

    /// Nudge the focused DAC EQ band one step (1.0 dB). Emits only when the value moved, so a tap
    /// past either end does not run the helper for nothing.
    /// One step up or down for the selected volume-curve point, held between its neighbours.
    fn vol_curve_nudge(&mut self, dir: i32) -> Vec<Action> {
        let i = self.vol_curve_sel;
        let was = self.vol_curve[i];
        let want = (was as i32 + dir).clamp(0, crate::vol_curve::MAX as i32) as u8;
        crate::vol_curve::set(&mut self.vol_curve, i, want);
        if self.vol_curve[i] == was {
            return vec![];
        }
        self.vol_curve_on = true;
        vec![Action::VolCurveChanged]
    }

    /// The volume curve for the shell: `Some(points)` when the owner's curve is in use.
    pub fn vol_curve(&self) -> Option<[u8; crate::vol_curve::POINTS]> {
        self.vol_curve_on.then_some(self.vol_curve)
    }
    /// The edited points, in use or not, for the settings file.
    pub fn vol_curve_points(&self) -> ([u8; crate::vol_curve::POINTS], bool) {
        (self.vol_curve, self.vol_curve_on)
    }
    /// Restore the curve at boot. A curve the helper would refuse is ignored.
    pub fn restore_vol_curve(&mut self, pts: [u8; crate::vol_curve::POINTS], on: bool) {
        if crate::vol_curve::valid(&pts) {
            self.vol_curve = pts;
            self.vol_curve_on = on;
        }
    }
    /// The shell's report: is `cinder-voltable` installed?
    pub fn set_vol_curve_available(&mut self, ok: bool) {
        self.vol_curve_ok = ok;
    }

    fn dac_eq_nudge(&mut self, dir: i32) -> Vec<Action> {
        let g = &mut self.dac_eq[self.dac_eq_sel];
        let want = if dir > 0 {
            (*g + crate::dac_eq::BAND_STEP).min(crate::dac_eq::BAND_MAX)
        } else {
            (*g - crate::dac_eq::BAND_STEP).max(crate::dac_eq::BAND_MIN)
        };
        if want == *g {
            return vec![];
        }
        *g = want;
        vec![Action::DacEqChanged]
    }

    /// Left/Right on the Sound screen. On the slider row it nudges the balance a step; on every
    /// other row it toggles, which is what it did before the slider existed.
    fn sound_lr(&mut self, dir: i32) -> Vec<Action> {
        if self.sound_sel != crate::sound::ROW_BALANCE {
            return self.sound_toggle_row();
        }
        const STEP: i32 = 5;
        let want =
            (self.snd_balance as i32 + dir * STEP).clamp(0, crate::sound::BALANCE_MAX as i32);
        if want as usize == self.snd_balance {
            return vec![];
        }
        self.snd_balance = want as usize;
        vec![Action::BalanceChanged]
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
                crate::confirm::Hit::Cancel => {
                    self.pending_play = None;
                    self.pending_rest.clear();
                    vec![]
                }
                // The menu's own rows say which action was chosen; a yes/no card's Confirm means
                // "the thing the card is named after".
                crate::confirm::Hit::Restart => vec![Action::Restart],
                crate::confirm::Hit::PowerOff => vec![Action::PowerOff],
                // "Replace Up Next": the new sequence replaces the list, which is what starting
                // something does anyway. No `QueueChanged` first — the play action rebuilds the
                // whole sequence, and an edit owed against the old one would only be a stale flush.
                crate::confirm::Hit::ClearQueue => self
                    .pending_play
                    .take()
                    .map(|act| vec![act])
                    .unwrap_or_default(),
                // The playing song was tapped again: queue the rest of its list behind it, and
                // leave playback alone.
                crate::confirm::Hit::RestNext => {
                    self.pending_play = None;
                    let rest = std::mem::take(&mut self.pending_rest);
                    if rest.is_empty() {
                        self.notify(crate::i18n::text("Nothing comes after this song"));
                        return vec![];
                    }
                    let n = rest.len().to_string();
                    self.notify_format("Playing next — {} songs", vec![("0", n)]);
                    self.insert_rows(rest, QueueAt::Next)
                }
                crate::confirm::Hit::RestRestart => {
                    self.pending_rest.clear();
                    match self.pending_play.take() {
                        Some(act) => self.start_play_now(act),
                        None => vec![],
                    }
                }
                crate::confirm::Hit::KeepQueue => {
                    // Honour it for real: what was still to come plays AFTER the new sequence.
                    self.queue_keep = true;
                    self.pending_play
                        .take()
                        .map(|act| vec![act])
                        .unwrap_or_default()
                }
                crate::confirm::Hit::Confirm => match ask {
                    crate::confirm::Ask::ClearQueue => {
                        self.notify(crate::i18n::text("Up Next cleared"));
                        self.queue_clear()
                    }
                    crate::confirm::Ask::Restart => vec![Action::Restart],
                    crate::confirm::Ask::PowerOff => vec![Action::PowerOff],
                    crate::confirm::Ask::SonyReceiver => vec![Action::BootToSonyReceiver],
                    crate::confirm::Ask::ResetSettings => self.reset_settings(),
                    crate::confirm::Ask::DeletePlaylist => {
                        match self.playlist_row().map(|p| p.id) {
                            Some(id) => {
                                // Leave the page first: it is about to stop existing, and the row
                                // index it is holding would point at whatever slid into its place.
                                self.pop();
                                self.notify(crate::i18n::text("Playlist deleted"));
                                vec![Action::PlaylistDelete(id)]
                            }
                            None => vec![],
                        }
                    }
                    crate::confirm::Ask::DeleteView => self.delete_edited_view(),
                    crate::confirm::Ask::DeleteTrack | crate::confirm::Ask::DeleteAlbum => {
                        self.delete_confirmed()
                    }
                    crate::confirm::Ask::WifiUpdate => {
                        self.update_running = true;
                        self.update_install_note =
                            crate::i18n::text("Starting the download…").to_string();
                        vec![Action::WifiUpdateInstall]
                    }
                    crate::confirm::Ask::WifiForget => {
                        self.wifi_status.state = crate::wifi::State::Busy;
                        self.wifi_status.note.clear();
                        self.wifi_networks.clear();
                        crate::wifi::note_state(crate::wifi::State::Busy);
                        vec![Action::WifiForget]
                    }
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
        // The pull-down panel owns all taps while it is open, like the Shelf below.
        if self.quick_open {
            return self.quick_tap(x, y);
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
        // Onboarding owns the screen: right ~60% = next/finish, left = previous page. On "Make it
        // yours" a row is a switch first; the rest of the page still turns it.
        if matches!(self.current(), Screen::Onboarding) {
            if self.onboarding_page == crate::onboarding::CHOICES_PAGE {
                if let Some(choice) = crate::onboarding::choice_at(y) {
                    return self.onboarding_choice(choice);
                }
            }
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
                self.set_playing_optimistic(!self.playing);
                return vec![Action::PlayPause];
            }
            self.go(Screen::NowPlaying);
            return vec![];
        }
        // Global chrome (status bar, the full top strip). The bookmark glyph opens the **Shelf**;
        // tapping anywhere else on the bar opens the **Menu** — the rest of the strip stays one big
        // forgiving Menu target. Both zones come from `chrome::status_hit`, which is built from the
        // same constants that place the glyphs. Header back chevron → Back (below).
        // The Library's SORT/ORDER chip, view button and search button claim the strip's bottom
        // edge above them — see `library::SORT_HIT_TOP`. Only there: `tap_library` handles them.
        let sort_chip = self.current() == Screen::Library
            && (library::hit_sort_chip(self.lib_tab, x, y, self.search_enabled)
                || library::hit_view_button(x, y, self.search_enabled)
                || (self.search_enabled && library::hit_search(x, y)));
        match crate::chrome::status_hit(x, y).filter(|_| !sort_chip && !self.landscape()) {
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
            Some(crate::chrome::StatusTap::Nothing) => return vec![],
            None => {}
        }
        // Back chevron: a generous ≥44px target (the whole header-left block, from just under
        // the status strip to the header rule) on every screen that draws one.
        //
        // THE MENU DRAWS ONE. It calls `chrome::header("Menu", …)` like every other screen, and
        // `chrome::header` unconditionally draws the chevron — but the Menu was excluded here, so
        // the glyph sat on the glass and did nothing. On a device whose only input is a finger,
        // a visible affordance that ignores it is worse than no affordance: the hardware Back key
        // and the status strip's Now Playing zone both worked, so the one thing that LOOKED like
        // the way out was the one thing that was not. Found by the 2026-09-06 UI audit.
        //
        // Safe on the Menu specifically because `menu::TOP == HEADER_BOTTOM`: no row is drawn in
        // the band this claims, so nothing is being taken away from the list.
        //
        // Now Playing and Lock stay out because they genuinely draw no header — they are the
        // roots, and `pop()` on a one-entry stack is a no-op, but drawing nothing and claiming the
        // target anyway is how an invisible control gets created.
        let has_header = !matches!(self.current(), Screen::NowPlaying | Screen::Lock);
        if has_header && crate::chrome::back_hit(x, y) {
            // On a Settings topic page the arrow is the same Back as the edge swipe: the home page.
            if self.current() == Screen::Settings && self.settings_page.is_some() {
                self.settings_page_back();
            } else {
                self.pop();
            }
            return vec![];
        }

        match self.current() {
            Screen::Menu => {
                if crate::menu::strip_hit(y) {
                    // The strip names what is playing; tapping it goes there. Popping (rather than
                    // pushing) when Now Playing is underneath keeps Back from bouncing between the
                    // two.
                    if self.stack.len() >= 2
                        && self.stack[self.stack.len() - 2] == Screen::NowPlaying
                    {
                        self.pop();
                    } else {
                        self.go(Screen::NowPlaying);
                    }
                    return vec![];
                }
                // Under the strip the page scrolls (Big view); ask in page coordinates.
                if y < crate::menu::TOP {
                    return vec![];
                }
                let y = y + self.menu_scroll();
                if let Some(i) = crate::menu::home_chip_at(x, y, self.menu_len()) {
                    self.home_screen = i as u8;
                    return vec![];
                }
                if let Some(row) = crate::menu::row_at(y, self.menu_len()) {
                    self.menu_idx = row;
                    return self.activate_menu(row);
                }
                vec![]
            }
            Screen::NowPlaying if self.winamp_view() => self.winamp_tap(x, y),
            Screen::NowPlaying if self.tape_view() => {
                use crate::tape::Tap;
                let (lx, ly) = crate::tape::to_land(x, y);
                match crate::tape::tap_at(lx, ly) {
                    Tap::Menu => {
                        self.push(Screen::Menu);
                        vec![]
                    }
                    Tap::Prev => vec![Action::Prev],
                    Tap::Next => vec![Action::Next],
                    Tap::PlayPause => {
                        self.set_playing_optimistic(!self.playing);
                        vec![Action::PlayPause]
                    }
                }
            }
            Screen::NowPlaying => {
                // One question, asked of the same layout the style draws from
                // (`now_playing::layout`): no coordinate for this screen lives in `nav`. The
                // layout's order carries the priorities that used to be this function's order —
                // the Lyrics chip before the Menu band it sits in, the heart before the title
                // block it overlaps, the title block before the transport.
                use crate::now_playing::Hit;
                // Settings ▸ Back arrow: ahead of the layout, whose Menu band it sits in. It does
                // what the left-edge swipe does.
                if crate::now_playing::hit_back(x, y) {
                    return self.press(Button::Back);
                }
                let layout =
                    crate::now_playing::layout(self.style, self.night, self.lyrics_button());
                match layout.hit(x, y) {
                    Some(Hit::Lyrics) => {
                        self.open_lyrics();
                        vec![]
                    }
                    Some(Hit::Like) => vec![Action::ToggleLiked],
                    // The metadata block IS the button for "tell me more about this file". Sony
                    // puts the same screen behind an options menu; Cinder has none on Now Playing,
                    // and hanging it off the text the question is about needs no new furniture.
                    Some(Hit::Info) => {
                        self.track_info_scroll_px = 0;
                        self.push(Screen::TrackInfo);
                        vec![]
                    }
                    Some(Hit::PlayPause) => {
                        self.set_playing_optimistic(!self.playing);
                        vec![Action::PlayPause]
                    }
                    Some(Hit::Prev) => vec![Action::Prev],
                    Some(Hit::Next) => vec![Action::Next],
                    Some(Hit::Shuffle) => vec![Action::ShuffleToggle],
                    Some(Hit::Repeat) => vec![Action::RepeatCycle],
                    Some(Hit::Toolbar(slot)) => {
                        match slot {
                            0 => self.open_browse(),
                            1 if self.zappin_active => self.push(Screen::Zappin),
                            1 => self.push(Screen::UpNext),
                            2 => match self.np_button {
                                crate::now_playing::NpButton::Bluetooth => {
                                    self.push(Screen::Bluetooth)
                                }
                                crate::now_playing::NpButton::Sound => self.push(Screen::Sound),
                                crate::now_playing::NpButton::Eq => self.push(Screen::Eq),
                                crate::now_playing::NpButton::Menu => self.push(Screen::Menu),
                            },
                            _ => self.push(Screen::Settings),
                        }
                        vec![]
                    }
                    // The band every OTHER screen gives to its back chevron. Now Playing is a root
                    // and draws no chevron, so the band goes to the Menu, which is what the status
                    // strip above it already means.
                    Some(Hit::Menu) => {
                        self.push(Screen::Menu);
                        vec![]
                    }
                    None => vec![],
                }
            }
            Screen::Zappin => match crate::retro_preview::hit(
                x,
                y,
                self.zappin_active,
                self.context_idx() < self.context().len(),
            ) {
                Some(crate::retro_preview::Hit::Short) => {
                    self.zappin_preview_ms = 4_000;
                    vec![Action::ZappinLength(4_000)]
                }
                Some(crate::retro_preview::Hit::Long) => {
                    self.zappin_preview_ms = 15_000;
                    vec![Action::ZappinLength(15_000)]
                }
                Some(crate::retro_preview::Hit::Prev) if self.zappin_active => {
                    vec![Action::ZappinPrev]
                }
                Some(crate::retro_preview::Hit::Next) if self.zappin_active => {
                    vec![Action::ZappinNext]
                }
                Some(crate::retro_preview::Hit::PlayPause) if self.zappin_active => {
                    self.set_playing_optimistic(!self.playing);
                    vec![Action::PlayPause]
                }
                Some(crate::retro_preview::Hit::PlayPause)
                    if self.context_idx() < self.context().len() =>
                {
                    vec![Action::ZappinStart(self.zappin_preview_ms)]
                }
                Some(crate::retro_preview::Hit::Full) if self.zappin_active => {
                    vec![Action::ZappinFull]
                }
                Some(crate::retro_preview::Hit::End) if self.zappin_active => {
                    vec![Action::ZappinEnd]
                }
                _ => vec![],
            },
            Screen::Folders => self.tap_folders(x, y),
            Screen::Books => {
                if !self.lib.books.is_empty() && crate::chrome::mode_hit(x, y) {
                    self.books_to_music();
                    return vec![];
                }
                self.tap_books(y)
            }
            Screen::SensMe => self.tap_sensme(x, y),
            Screen::Library => self.tap_library(x, y),
            Screen::Album => {
                // track rows via the render-mirroring hit test (rows start 312 @56 —
                // library::album_view geometry; the Play-album band above returns None).
                if let Some(album) = self
                    .lib
                    .albums_flat()
                    .get(self.album_view)
                    .map(|a| (*a).clone())
                {
                    if let Some(row) = library::album_hit_track(&album, self.album_scroll_px, y) {
                        self.album_track_idx = row;
                        return self.play_album_row(&album, row);
                    }
                    if self.opt(Opt::DeleteMusic) && crate::chrome::header_action_hit(x, y) {
                        let ids = album.track_list.iter().map(|s| s.object_id).collect();
                        self.ask_delete(ids, true);
                        return vec![];
                    }
                    // The "Play album" band: play from track 1 — the shell's album_context
                    // expands it to the whole album in order. (Hit-tested through the band's own
                    // rect; the old literal range started 16px above where the band is drawn.)
                    // Shuffle, beside it: this album, its tracks shuffled. One album is one group,
                    // so it deals songs whatever Settings ▸ Shuffle says.
                    if library::hit_album_add(x, y) {
                        return self.enqueue_album_at(&album, y, QueueAt::Later);
                    }
                    if library::hit_album_shuffle(x, y) {
                        let ids: Vec<i64> = album.track_list.iter().map(|s| s.object_id).collect();
                        if ids.is_empty() {
                            return vec![];
                        }
                        self.play_list = ids;
                        self.list_shuffle_by = crate::shuffle::ShuffleBy::Songs;
                        return self.start_play_action(Action::ShuffleList);
                    }
                    if library::hit_album_play_band(x, y) {
                        self.album_track_idx = 0;
                        let id = album.track_list.first().map(|s| s.object_id);
                        return id.map(|i| self.start_play(i)).unwrap_or_default();
                    }
                }
                vec![]
            }
            // Track information: the pinned action band, plus the link rows.
            Screen::TrackInfo => {
                if self.opt(Opt::DeleteMusic) && crate::chrome::header_action_hit(x, y) {
                    if let Some(id) = self
                        .playing_object_id()
                        .filter(|_| !self.track_info.is_empty())
                    {
                        self.ask_delete(vec![id], false);
                    }
                    return vec![];
                }
                if !self.presets.is_empty() && crate::track_info::hit_album_sound(x, y) {
                    return self.album_sound_step(false);
                }
                if !self.presets.is_empty() && crate::track_info::hit_song_sound(x, y) {
                    return self.album_sound_step(true);
                }
                if crate::track_info::hit_add_to_playlist(y) {
                    if let Some(oid) = self.playing_object_id() {
                        self.open_playlist_pick(oid);
                    } else {
                        self.notify(crate::i18n::text("Nothing is playing"));
                    }
                    return vec![];
                }
                // The Rating row: its five stars are the control. Tapping the star already set
                // clears the rating, so there is a way back to "not rated".
                let on_rating =
                    crate::track_info::hit_row(&self.track_info_rows, self.track_info_scroll_px, y)
                        .and_then(|i| self.track_info.get(i))
                        .is_some_and(|(label, _)| crate::track_info::is_rating(label));
                if on_rating {
                    return match crate::track_info::star_at(x) {
                        Some(stars) => self.rate_playing(stars),
                        None => vec![],
                    };
                }
                // "What is this part of?" — answered by going there, rather than by making the
                // user leave, open Library and find it by hand.
                let hit =
                    crate::track_info::hit_row(&self.track_info_rows, self.track_info_scroll_px, y)
                        .and_then(|i| self.track_info.get(i))
                        .filter(|(label, value)| {
                            crate::track_info::is_link(label) && !value.is_empty()
                        })
                        .map(|(label, value)| (label.clone(), value.clone()));
                if let Some((label, value)) = hit {
                    let went = match label.as_str() {
                        "Lyrics" => {
                            self.open_lyrics();
                            true
                        }
                        "Artist" | "Album artist" => self.open_artist_named(&value),
                        _ => self.open_album_named(&value),
                    };
                    // A tag can name something the library does not have under that spelling.
                    // Say so rather than swallowing the tap, which reads as a dead control.
                    if !went {
                        self.notify(crate::i18n::text("Not in the library"));
                    }
                }
                vec![]
            }
            Screen::Artist => self.tap_artist(x, y),
            Screen::Playlist => self.tap_playlist(x, y),
            Screen::Keyboard => self.tap_keyboard(x, y),
            Screen::PlaylistPick => self.tap_playlist_pick(y),
            Screen::TrackPick => self.tap_track_pick(y),
            Screen::PlaylistEdit => self.tap_playlist_edit(x, y),
            Screen::PlaylistCover => self.tap_cover_pick(y),
            Screen::ViewEdit => self.tap_view_edit(x, y),
            Screen::Search => self.tap_search(y),
            Screen::UpNext => {
                use crate::up_next::Slot;
                // CLEAR empties what is still to come and is only drawn when something is.
                if self.upcoming_len() > 0 && crate::up_next::hit_clear_chip(x, y) {
                    // ASK. See `confirm::Ask::ClearQueue` for why this is not a bare tap.
                    self.confirm = Some(crate::confirm::Ask::ClearQueue);
                    return vec![];
                }
                // MIX. With shuffle OFF it turns shuffle on — the same as the transport's shuffle
                // button, so the icon on Now Playing tells the truth about the order. With shuffle
                // already on it deals the list again.
                if self.context.len() >= 3 && crate::up_next::hit_shuffle_chip(x, y) {
                    if !self.shuffle_on {
                        return vec![Action::ShuffleToggle];
                    }
                    return self.queue_shuffle();
                }
                // CLEAR on the PREVIOUSLY PLAYED heading. Tested before the slot dispatch below,
                // for the same reason the chips are: the heading is not otherwise tappable, so
                // nothing else wants this pixel.
                if crate::up_next::hit_history_clear(
                    &self.up_next_layout(),
                    x,
                    y,
                    self.queue_scroll_px,
                ) {
                    self.notify(crate::i18n::text("History cleared"));
                    return self.history_clear();
                }
                // SAVE on the NOW PLAYING heading: Up Next as a playlist, named on the keyboard.
                if crate::up_next::hit_now_save(&self.up_next_layout(), x, y, self.queue_scroll_px)
                {
                    self.open_keyboard(KbPurpose::SaveQueue);
                    return vec![];
                }
                // The grab-handle column belongs to the reorder drag. Swallowed rather than
                // treated as a tap: playing a track because a reorder came out too short to
                // classify is a nasty surprise.
                if crate::up_next::queue_grip_hit(x)
                    && matches!(
                        self.up_next_layout().at(y, self.queue_scroll_px),
                        Some(Slot::History(_)) | Some(Slot::Upcoming(_))
                    )
                {
                    return vec![];
                }
                match self.up_next_layout().at(y, self.queue_scroll_px) {
                    // The playing row is not a destination; it is where you already are.
                    Some(Slot::Current(_)) => {
                        self.go(Screen::NowPlaying);
                        vec![]
                    }
                    // An upcoming row is an index into the SEQUENCE, so it jumps within it.
                    //
                    // `PlayContextAt`, not `PlayIndex`: the row is an index into the sequence on
                    // screen, and `PlayIndex` would resolve the tapped track's ALBUM instead —
                    // which after "Shuffle all songs" replaced a shuffled library with one album.
                    // The list you are looking at is the list you get.
                    //
                    // NOT through `start_play_action`: a jump WITHIN the list replaces nothing, so
                    // there is nothing to ask about.
                    Some(Slot::Upcoming(i)) if i < self.context.len() => {
                        // Playing something new re-arms the follow, so the list snaps to the new
                        // track instead of staying where the finger left it.
                        self.queue_follow = true;
                        vec![Action::PlayContextAt(i)]
                    }
                    // A HISTORY row is not a sequence index — it can have come from a different
                    // album or a different shuffle. So it is resolved by IDENTITY, and only falls
                    // back to "play this song" when it is genuinely no longer part of what is
                    // playing:
                    //
                    //   * still in the context, behind the playing row → `PlayContextAt`, which
                    //     keeps the shuffled sequence exactly as it is on screen. This is the
                    //     common case — walking back through the album you are in — and it must
                    //     behave precisely as it did before the history became a real list.
                    //   * anything else → `PlayIndex`, i.e. play that song and let its own album
                    //     follow it. There is no sequence on screen to jump within.
                    Some(Slot::History(i)) => {
                        let Some(row) = self.history.get(i) else {
                            return vec![];
                        };
                        let id = row.object_id;
                        self.queue_follow = true;
                        match self.context.iter().position(|t| t.object_id == id) {
                            Some(j) => vec![Action::PlayContextAt(j)],
                            None => self.start_play_action(Action::PlayIndex(id)),
                        }
                    }
                    Some(Slot::Upcoming(_)) => vec![],
                    // A section heading, or the empty state: keep the old shortcut home.
                    _ => {
                        self.go(Screen::NowPlaying);
                        vec![]
                    }
                }
            }
            Screen::GenreFilter => {
                if let Some(r) = library::genre_row_at(&self.lib, y, self.genre_scroll_px) {
                    let changed = match r {
                        // A TOGGLE, and the one row that does not close the sheet: Hi-Res and a
                        // genre AND together, so setting one and then the other has to be possible
                        // without walking back in.
                        library::GENRE_ROW_HIRES => {
                            self.lib.filter_hires = !self.lib.filter_hires;
                            true
                        }
                        // "All genres" clears the genre axis. Setting and clearing are the same
                        // gesture, so the screen needs no second control for it.
                        library::GENRE_ROW_ALL => {
                            let changed = self.lib.filter_genre.is_some();
                            self.lib.filter_genre = None;
                            changed
                        }
                        _ => {
                            let want = self
                                .lib
                                .genres
                                .get(r - library::GENRE_HEAD_ROWS)
                                .map(|g| g.id);
                            let changed = want != self.lib.filter_genre;
                            self.lib.filter_genre = want;
                            changed
                        }
                    };
                    if changed {
                        // The visible set just changed, so anything derived from it is stale: the
                        // A-Z map, the scroll offset (the list is shorter now) and any coasting fling.
                        self.az_memo = None;
                        self.lib_scroll_px = 0;
                        self.fling_v = 0.0;
                        self.lib_idx = 0;
                        self.album_expanded = None;
                    }
                    // Back to the list on a genre choice — you came here to answer one question.
                    // The Hi-Res toggle stays, because it is the one row you might want to combine.
                    if r != library::GENRE_ROW_HIRES {
                        self.pop();
                    }
                }
                vec![]
            }
            Screen::Settings => {
                if let Some(row) = crate::settings::row_at(
                    y,
                    self.settings_scroll_px,
                    self.settings_page,
                    self.settings_lang(),
                ) {
                    self.settings_sel = row;
                    return self.settings_activate();
                }
                vec![]
            }
            Screen::Help => {
                // The last row replays the first-run intro. Every other row is a fact.
                if crate::help::item_at(x, y, self.help_scroll_px, self.opt(Opt::Quick)).is_some() {
                    self.onboarding_page = 0;
                    self.push(Screen::Onboarding);
                }
                vec![]
            }
            Screen::Skins => {
                match crate::language::row_at(y, self.skins_scroll_px, self.skin_names.len()) {
                    Some(i) => {
                        self.pop();
                        vec![Action::PickSkin(i)]
                    }
                    None => vec![],
                }
            }
            Screen::Language => {
                // Stay on the page, like the palette picker: it repaints in the language just
                // picked, which is the proof it took.
                let n = self.languages.len();
                if let Some(i) = crate::language::row_at(y, self.language_scroll_px, n) {
                    self.language = self.languages[i];
                }
                vec![]
            }
            Screen::Palette => {
                use crate::palette_list::{item_at, Item};
                let order = self.palette_order();
                match item_at(
                    x,
                    y,
                    self.palette_scroll_px,
                    order.len(),
                    self.palette_skipped.len(),
                ) {
                    Some(Item::Sort(i)) => self.palette_sort = i,
                    // Stay on the page: the whole screen repaints in the new colours at once, which
                    // is the comparison you came here to make.
                    Some(Item::Entry(i)) => self.pick_palette(order[i]),
                    // A refused file or the ADD row says what to do; there is nothing to tap into.
                    Some(Item::Skipped(_)) | Some(Item::Add) | None => {}
                }
                vec![]
            }
            Screen::Display => {
                // A row scrolled up under the header is not there to tap.
                if y < crate::chrome::HEADER_BOTTOM {
                    return vec![];
                }
                let y = y + self.display_scroll();
                if let Some(i) = crate::display::night_chip_at(x, y) {
                    self.display_sel = crate::display::ROW_NIGHT;
                    return self.set_night_mode(i);
                }
                // A swatch tap picks that accent directly. Checked first because it lives inside
                // the Accent row's band, and falling through to `display_activate` would advance
                // the cycle by one instead of honouring the colour under the finger. Not while the
                // palette has its own accent: the swatches are not drawn then, and the row's tap
                // falls through to `display_activate`, which explains.
                let swatch = (!self.palette_pins_accent())
                    .then(|| crate::display::accent_hit(x, y))
                    .flatten();
                if let Some(i) = swatch {
                    self.display_sel = crate::display::ROW_ACCENT;
                    self.accent = Accent::from_index(i);
                    return vec![];
                }
                if let Some(i) = crate::display::style_chip_at(x, y) {
                    self.display_sel = crate::display::ROW_STYLE;
                    let style = crate::style::Style::from_index(i);
                    // Winamp or Cassette again: the skins or tapes on the drive, to pick one.
                    let skinned = matches!(
                        style,
                        crate::style::Style::Winamp | crate::style::Style::Cassette
                    );
                    if skinned && self.style == style {
                        self.skin_names.clear();
                        self.skins_scroll_px = 0;
                        self.push(Screen::Skins);
                        return vec![Action::ListSkins];
                    }
                    self.style = style;
                    return vec![];
                }
                if let Some(i) = crate::display::volume_chip_at(x, y) {
                    self.display_sel = crate::display::ROW_VOLUME;
                    self.volume_hud = i as u8;
                    self.show_volume_hud_sample();
                    return vec![];
                }
                if let Some(row) = crate::display::row_at(y) {
                    self.display_sel = row;
                    // The Size row is a SLIDER TRACK: the tap's x picks the stop directly (the
                    // SeekBar idiom) rather than cycling one step per tap.
                    if row == crate::display::ROW_SIZE {
                        crate::text::set_scale_idx(crate::display::size_idx_at(x));
                        return vec![Action::UiScaleChanged];
                    }
                    // The chips ARE the Night, Volume and Style rows; a tap on a margin chooses
                    // nothing.
                    if row == crate::display::ROW_NIGHT
                        || row == crate::display::ROW_VOLUME
                        || row == crate::display::ROW_STYLE
                    {
                        return vec![];
                    }
                    return self.display_activate();
                }
                vec![]
            }
            Screen::Wifi => self.tap_wifi(x, y),
            Screen::WifiNetworks => self.tap_wifi_list(x, y),
            Screen::NetRadio => self.tap_radio(x, y),
            Screen::WifiSpeakers => self.tap_speakers(x, y),
            Screen::MediaServers => self.tap_media(y),
            Screen::RadioFind => match crate::netradio::hit_found(x, y, self.radio_found.len()) {
                Some(i) if !self.radio_finding => vec![Action::RadioAdd(i)],
                _ => vec![],
            },
            Screen::WifiRemote => self.tap_remote(x, y),
            Screen::WifiOptions => self.tap_wifi_options(x, y),
            Screen::Alarm => {
                if crate::alarm::hit_button(x, y) {
                    self.alarm_on = !self.alarm_on;
                    self.notify(if self.alarm_on {
                        crate::i18n::text("Alarm on")
                    } else {
                        crate::i18n::text("Alarm off")
                    });
                } else if let Some((row, delta)) = crate::alarm::hit_btn(x, y) {
                    self.alarm_sel = row;
                    self.alarm_min = crate::alarm::step(self.alarm_min, row, delta);
                }
                vec![]
            }
            Screen::ClockSet => {
                if crate::clockset::hit_set(x, y) {
                    self.notify(crate::i18n::text("Clock set"));
                    return vec![Action::ClockSet];
                }
                // A tap on a row's label half only moves the cursor — reading the screen must not
                // change the time.
                if let Some((f, delta)) = crate::clockset::hit_btn(x, y) {
                    self.clock_sel = f;
                    crate::clockset::step(&mut self.clock_fields, f, delta);
                } else if let Some(f) = crate::clockset::field_at(y) {
                    self.clock_sel = f;
                }
                vec![]
            }
            Screen::Sound => {
                // A/B compare control (top-right of the header). Geometry comes from `sound` so the
                // band and the drawn boxes cannot drift apart — they had, by 4px, on top of the
                // segments being too small to hit at all ("the a/b buttons are hard to press").
                // Tapping a segment SELECTS it rather than flipping the pair: on a control this
                // size "tap B to hear B" is what a finger expects, and it makes a double-tap
                // idempotent instead of a round trip.
                let nc = crate::sound::nc_top(self.sound_scroll());
                if self.nc_plug
                    && y >= crate::sound::TOP
                    && (nc..nc + crate::sound::NC_H()).contains(&y)
                {
                    self.push(Screen::Nc);
                    return vec![];
                }
                if let Some(seg) = crate::sound::hit_ab(x, y) {
                    // A and B are two SETUPS now, not "effects on" vs "effects bypassed" — see
                    // SoundSetup. Selecting the live one is a no-op rather than a round trip.
                    return self.select_setup(seg);
                }
                // CENTRE reset, before the row dispatch: it sits inside the Balance row, so the
                // row's own handling would otherwise swallow it.
                // MONO, left of CENTRE in the same row. Tested BEFORE the reset, because the two
                // sit side by side and the slack in neither may reach the other.
                if crate::sound::hit_balance_mono(x, y, self.sound_scroll()) {
                    self.sound_sel = crate::sound::ROW_BALANCE;
                    self.mono = !self.mono;
                    return vec![Action::MonoChanged];
                }
                if crate::sound::hit_balance_reset(x, y, self.sound_scroll()) {
                    self.sound_sel = crate::sound::ROW_BALANCE;
                    // Dead under mono, and the row draws it that way: with one signal in both
                    // channels there is no image to re-centre.
                    if self.mono || self.snd_balance == crate::sound::BALANCE_CENTRE {
                        return vec![];
                    }
                    self.snd_balance = crate::sound::BALANCE_CENTRE;
                    return vec![Action::SoundChanged];
                }
                if let Some(mode) = crate::sound::hit_mode(
                    x,
                    y,
                    self.sound_scroll(),
                    crate::sound::mode_count(self.clearaudio_ok),
                ) {
                    return self.set_sound_mode(mode);
                }
                if let Some(row) = crate::sound::row_at(y, self.sound_scroll()) {
                    self.sound_sel = row;
                    // On the slider row a tap inside the grab band JUMPS the knob to the finger
                    // (the same gesture as a drag that never moved), while a tap on the label half
                    // only moves focus — so reading the row can't silently change the setting.
                    if row == crate::sound::ROW_BALANCE {
                        if crate::sound::balance_grab(y, self.sound_scroll()) {
                            let want = crate::sound::balance_at(x);
                            if want != self.snd_balance {
                                self.snd_balance = want;
                                return vec![Action::BalanceChanged];
                            }
                        }
                        return vec![];
                    }
                    return self.sound_toggle_row();
                }
                vec![]
            }
            Screen::Fm => {
                use crate::fm::Hit;
                // A scan owns the tuner for about a minute; taking taps during it would queue
                // retunes behind a job that is already retuning. Only Scan responds, and it cancels.
                if self.fm_scanning {
                    return match crate::fm::hit(x, y, self.fm_rec_available) {
                        Some(Hit::Scan) => {
                            self.fm_scanning = false;
                            vec![Action::FmScan]
                        }
                        _ => vec![],
                    };
                }
                match crate::fm::hit(x, y, self.fm_rec_available) {
                    Some(Hit::Power) => {
                        self.fm_playing = !self.fm_playing;
                        vec![Action::FmPower(self.fm_playing)]
                    }
                    Some(Hit::Step(d)) => {
                        self.fm_khz =
                            (self.fm_khz + d).clamp(crate::fm::MIN_KHZ, crate::fm::MAX_KHZ);
                        vec![Action::FmTune(self.fm_khz)]
                    }
                    Some(Hit::Dial(k)) => {
                        self.fm_khz = k;
                        vec![Action::FmTune(k)]
                    }
                    Some(Hit::Prev) => vec![Action::FmSeek(-1)],
                    Some(Hit::Next) => vec![Action::FmSeek(1)],
                    Some(Hit::Preset(i)) => {
                        if i < self.fm_n_stations {
                            self.fm_khz = self.fm_stations[i];
                            vec![Action::FmTune(self.fm_khz)]
                        } else {
                            vec![]
                        }
                    }
                    Some(Hit::BtOut) => {
                        self.fm_bt_out = !self.fm_bt_out;
                        vec![Action::FmBtOut(self.fm_bt_out)]
                    }
                    // The shell owns the recording; this only asks. REC lights when the shell
                    // reports seconds, so a request that fails never shows as recording.
                    Some(Hit::Rec) => vec![Action::FmRecord(self.fm_rec_secs < 0)],
                    Some(Hit::Scan) => {
                        self.fm_scanning = true;
                        self.fm_scan_pct = 0;
                        vec![Action::FmScan]
                    }
                    None => vec![],
                }
            }
            Screen::Device => {
                // One live control on the screen; everything else is a readout, so a tap anywhere
                // but the care row is deliberately inert rather than selecting a row that cannot
                // do anything. The hit follows the scroll through the same layout walk that drew it.
                if crate::device::hit_care(&self.device_view(), y, self.device_scroll_px) {
                    return self.battery_care_toggle();
                }
                vec![]
            }
            Screen::Advanced => {
                if let Some(row) =
                    crate::advanced::row_at(y, self.clear_bass_available, self.adv_scroll_px)
                {
                    self.adv_sel = row;
                    return self.advanced_toggle_row();
                }
                vec![]
            }
            Screen::VizSet => {
                if let Some(row) = crate::vizset::row_at(y) {
                    self.viz_sel = row;
                    return self.vizset_cycle_row();
                }
                vec![]
            }
            Screen::Soundscape => self.soundscape_tap(x, y),
            Screen::Tone => {
                // Same idiom as the Equalizer's band field, through `tone`'s own layout helpers:
                // tap a column to focus it, above the zero line raises, below lowers.
                if (crate::tone::FIELD_TOP..crate::tone::FIELD_BOTTOM).contains(&y) {
                    if let Some(band) = crate::tone::band_at(x) {
                        self.tone_sel = band;
                        return self.tone_nudge(if y < crate::tone::FIELD_MID { 1 } else { -1 });
                    }
                    return vec![];
                }
                if crate::tone::reset_at(x, y) {
                    if self.tone_bands == [0; crate::tone::BANDS] {
                        return vec![];
                    }
                    self.tone_bands = [0; crate::tone::BANDS];
                    return vec![Action::SoundChanged];
                }
                vec![]
            }
            Screen::Presets => {
                if let Some(h) =
                    crate::presets::hit(x, y, self.presets.len(), self.presets_scroll_px)
                {
                    return self.presets_tap(h);
                }
                vec![]
            }
            // No action: the shell reads `nc_items` every second while the earphones are in, and
            // the tap's settings save keeps the choice.
            Screen::Nc => {
                match crate::nc::hit(y, self.nc_model, self.nc_ambient_ok) {
                    Some(crate::nc::Hit::Mode(m)) => self.nc_mode = m,
                    Some(crate::nc::Hit::Model(i)) => self.nc_model = i,
                    None => {}
                }
                vec![]
            }
            Screen::DacEq => {
                // The Tone Control idiom through `dac_eq`'s own layout: tap a column to focus it,
                // above the zero line raises, below lowers.
                if (crate::dac_eq::FIELD_TOP..crate::dac_eq::FIELD_BOTTOM).contains(&y) {
                    if let Some(band) = crate::dac_eq::band_at(x) {
                        self.dac_eq_sel = band;
                        return self.dac_eq_nudge(if y < crate::dac_eq::FIELD_ZERO {
                            1
                        } else {
                            -1
                        });
                    }
                    return vec![];
                }
                if let Some(i) = crate::dac_eq::preset_at(x, y) {
                    let want = crate::dac_eq::PRESETS[i].1;
                    if self.dac_eq == want {
                        return vec![];
                    }
                    self.dac_eq = want;
                    return vec![Action::DacEqChanged];
                }
                if crate::dac_eq::reset_at(x, y) {
                    if self.dac_eq == [0; crate::dac_eq::BANDS] {
                        return vec![];
                    }
                    self.dac_eq = [0; crate::dac_eq::BANDS];
                    return vec![Action::DacEqChanged];
                }
                if crate::dac_eq::fit_at(x, y) {
                    let fitted = crate::dac_eq::fit(self.eq_bands);
                    self.notify(crate::i18n::text(
                        "Your 10-band curve, fitted to the DAC's five bands",
                    ));
                    if fitted == self.dac_eq {
                        return vec![];
                    }
                    self.dac_eq = fitted;
                    return vec![Action::DacEqChanged];
                }
                // Which equalizer the jack uses. The DAC curve itself does not move, only whether
                // Sony's 10-band is flat on the jack, so the chain is what needs re-applying.
                if crate::dac_eq::jack_row_at(y) {
                    self.jack_eq_dac = !self.jack_eq_dac;
                    return vec![Action::SoundChanged];
                }
                vec![]
            }
            Screen::VolCurve => {
                use crate::vol_curve as vc;
                if !self.vol_curve_ok {
                    self.notify(crate::i18n::text(
                        "Needs the installer's volume curve option",
                    ));
                    return vec![];
                }
                if (vc::FIELD_TOP..vc::FIELD_BOTTOM).contains(&y) {
                    // A tap above the knob raises the point by one step, below lowers it.
                    if let Some(i) = vc::point_at(x) {
                        self.vol_curve_sel = i;
                        return self.vol_curve_nudge(if y < vc::knob_y(self.vol_curve[i]) {
                            1
                        } else {
                            -1
                        });
                    }
                    return vec![];
                }
                let preset = if vc::reset_at(x, y) {
                    Some(vc::STRAIGHT)
                } else if vc::every_at(x, y) {
                    Some(vc::EVERY_STEP)
                } else {
                    None
                };
                if let Some(p) = preset {
                    if p == self.vol_curve && self.vol_curve_on {
                        return vec![];
                    }
                    self.vol_curve = p;
                    self.vol_curve_on = true;
                    return vec![Action::VolCurveChanged];
                }
                if vc::switch_at(y) {
                    self.vol_curve_on = !self.vol_curve_on;
                    return vec![Action::VolCurveChanged];
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
                match crate::bluetooth::hit(x, y, self.bt_on, self.bt_paired.len()) {
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
                    BtHit::Receiver => {
                        // Always arrives OFF: a switch left on by a page that was popped is not
                        // what the shell is running any more.
                        self.rx_on = false;
                        self.push(Screen::Receiver);
                        vec![]
                    }
                    // Connect straight from this screen — the whole point of the redesign. Same
                    // two-way rule the Devices screen uses: the connected row hangs up.
                    BtHit::PairedRow(i) => match self.bt_paired.get(i) {
                        Some(d) if d.connected => vec![Action::BtDisconnect],
                        Some(_) => self.bt_connect_row(i),
                        None => vec![],
                    },
                    BtHit::Advanced => {
                        self.push(Screen::BtCodec);
                        vec![]
                    }
                    BtHit::DebugLog => {
                        // No toast here: whether the capture reached the drive is the shell's to
                        // say (`cinder_toast`), and it can fail — the stack's file may not be
                        // readable to this app.
                        self.bt_debug_log = !self.bt_debug_log;
                        vec![Action::BtDebugLog]
                    }
                    // Codec-page rows: this screen's hit map cannot produce them.
                    BtHit::FineVolume | BtHit::None => vec![],
                }
            }
            Screen::BtCodec => {
                use crate::bluetooth::BtHit;
                // A row scrolled up under the header is not there to tap.
                if y < crate::chrome::HEADER_BOTTOM {
                    return vec![];
                }
                match crate::bluetooth::hit_codec(
                    x,
                    y + self.bt_codec_scroll(),
                    self.bt_on,
                    crate::bluetooth::shows_ldac(self.bt_codec),
                    self.bt_aac,
                    self.bt_sbc_xq,
                ) {
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
                    // Render-only from the shell's point of view: the trim itself is picked up from
                    // `cinder_get_bt_trim_half_db` on the next housekeeping tick, so cycling the row
                    // needs no action and cannot interrupt anything mid-list.
                    BtHit::FineVolume => {
                        self.cycle_bt_fine();
                        vec![]
                    }
                    _ => vec![],
                }
            }
            Screen::Receiver => {
                use crate::pairing::PairHit;
                // The pairing code a PC or phone is waiting on is answered HERE — the receiver's
                // whole point is being paired to from the other side.
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
                if crate::receiver::switch_hit(x, y) {
                    if self.bt_on || self.rx_on {
                        self.rx_on = !self.rx_on;
                        return vec![Action::RxChanged];
                    }
                    self.notify("Bluetooth is off. Turn it on first.");
                    return vec![];
                }
                if let Some(pref) = crate::receiver::codec_pref_hit(x, y) {
                    if pref != self.rx_codec_pref {
                        self.rx_codec_pref = pref;
                        self.notify(if self.rx_on {
                            "Reconnect from your phone to apply."
                        } else {
                            "Saved; applies when receiver is on."
                        });
                        return vec![Action::RxCodecChanged];
                    }
                    return vec![];
                }
                if crate::receiver::sony_hit(x, y) {
                    self.confirm = Some(crate::confirm::Ask::SonyReceiver);
                }
                vec![]
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
                match crate::pairing::hit(
                    x,
                    y,
                    self.bt_paired.len(),
                    self.bt_found.len(),
                    self.bt_paired_page,
                ) {
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
                            Some(_) => self.bt_connect_row(i),
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
                    PairHit::MorePaired => {
                        // Next page, wrapping. Disarms a pending FORGET like every other tap: the
                        // armed index is absolute, and leaving it armed across a page turn would
                        // put "TAP AGAIN" on a row the user cannot see.
                        self.bt_forget_armed = None;
                        self.bt_paired_page += 1;
                        vec![]
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
                    self.route_changed();
                    return vec![Action::UsbDacToggle(self.usb_dac_on)];
                }
                // Chosen before DAC mode starts, never during it, and locked once loaded.
                if self.usb_dac_ll_avail && !self.usb_dac_on && crate::usbdac::hit_low_latency(x, y)
                {
                    self.usb_dac_ll = !self.usb_dac_ll;
                    return vec![];
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
        // SEARCH, when the component is installed: a fixed button between the title and the chip.
        if self.search_enabled && library::hit_search(x, y) {
            self.open_search();
            return vec![];
        }
        if !self.lib.books.is_empty() && crate::chrome::mode_hit(x, y) {
            self.library_to_books();
            return vec![];
        }
        if library::hit_view_button(x, y, self.search_enabled) {
            self.cycle_lib_view();
            return vec![];
        }
        // SORT/ORDER chip in the header's right slot (right of the back chevron's x<80 band).
        // Songs cycles the SORT chip; Albums cycles the ORDER chip. Both reset the list position.
        if library::hit_sort_chip(self.lib_tab, x, y, self.search_enabled) {
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
        if (library::TAB_TOP..library::tab_hit_bottom(self.lib_band())).contains(&y) {
            // Resolve against the strip as it was LAST DRAWN. These used to be hardcoded x
            // thresholds that did not match the measured layout — at the default size "ALBUMS" is
            // drawn at x≈94..154, so tapping its left half selected SONGS, and the same drift ran
            // down the rest of the strip. Fixed thresholds could not have followed the UI scale
            // either. See `library::tab_layout`.
            let Some(slot) = self.tab_at_cached(x) else {
                return vec![];
            };
            // FOLDERS (Settings ▸ Folders tab) opens the folder browser; Back comes back here.
            let Some(tab) = slot else {
                self.open_folders();
                return vec![];
            };
            self.lib_tab = tab;
            self.lib_idx = 0;
            self.lib_scroll_px = 0;
            self.fling_v = 0.0;
            self.album_expanded = None;
            return vec![];
        }
        // The genre filter strip, above the list. Checked before the rows for the same reason the
        // A-Z rail is: it is not part of the list, so a tap here must never reach one.
        let band = self.lib_band();
        if library::filter_hit_at(self.lib_tab, y, band) {
            self.genre_scroll_px = 0;
            self.push(Screen::GenreFilter);
            return vec![];
        }
        // "NEW PLAYLIST", between the shuffle band and the list on the Playlists tab. Tested
        // before the rows for the same reason as the filter strip: it is not part of the list.
        match library::hit_new_at(self.lib_tab, x, y, band) {
            Some(library::NewKind::Playlist) => {
                self.open_keyboard(KbPurpose::NewPlaylist);
                return vec![];
            }
            Some(library::NewKind::Smart) => {
                self.open_view_edit(None);
                return vec![];
            }
            None => {}
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
                    self.lib_tab,
                    &self.lib,
                    letter,
                    self.lib_sort,
                    self.album_sort,
                    self.album_expanded,
                ) {
                    self.lib_scroll_px = px;
                    self.fling_v = 0.0; // a jump must not keep coasting from a previous flick
                }
            }
            return vec![];
        }
        // The accent band sits above the list on every tab, so test it before the rows (it is the
        // largest target on the screen; it used to be drawn but hit-tested nowhere).
        if library::hit_shuffle_band_at(x, y, band) {
            // THROUGH THE FUNNEL. A Shuffle band starts a new sequence, which means
            // `set_play_context` will clear the user's hand-built picks — so it has to ask first,
            // exactly as every Play path does. These four bands used to return the action
            // DIRECTLY and skip `start_play_action`, so the largest target on the screen was also
            // the only one that destroyed the queue without a word. On the Playlists tab the
            // effect was visible side by side: the PLAY band asked and the SHUFFLE band beside it
            // did not.
            return self.start_play_action(Action::Shuffle(match self.lib_tab {
                Tab::Songs => ShuffleScope::AllSongs,
                Tab::Albums => ShuffleScope::ByAlbum,
                Tab::Artists => ShuffleScope::ByArtist,
                Tab::Playlists => ShuffleScope::Playlist,
            }));
        }
        // Albums is a sortable accordion — its own hit test (expand/collapse, drill-in, play track).
        if matches!(self.lib_tab, Tab::Albums) {
            return self.tap_albums(x, y);
        }
        // The other tabs route through the render-mirroring hit test (library::hit_row): it knows
        // each tab's list top/row height and returns None for the shuffle band / gaps / off-list.
        let Some(row) = library::hit_item_at(
            self.lib_tab,
            &self.lib,
            self.lib_scroll_px,
            x,
            y,
            self.lib_band(),
        ) else {
            return vec![];
        };
        // A grid tile is one target: its cover and its name both open it. The play and shuffle
        // shortcuts at the right of a row have no place on a tile.
        let grid = library::view_of(&self.lib, self.lib_tab) == library::LibView::Grid;
        match self.lib_tab {
            // `row` is the RANK in the drawn (sorted) order — resolve through the same order.
            // The Songs list plays AS THE SONGS LIST, from the tapped row, in the order drawn.
            Tab::Songs => {
                self.lib_idx = row;
                let ids = self.songs_list_ids();
                self.start_play_list(ids, row)
            }
            // A playlist row opens that playlist's page, the same way an artist row opens theirs.
            // The button on the right of the row plays the whole list from the top in saved order
            // without the detour — the shortcut the row itself used to be.
            Tab::Playlists => {
                self.lib_idx = row;
                let Some(p) = self.lib.playlists.get(row) else {
                    return vec![];
                };
                if p.smart {
                    // A smart list is whatever its rules match NOW: recompute before using it, so
                    // a song played or rated since the tab was drawn is counted.
                    let id = p.id;
                    self.rebuild_smart(None);
                    let Some(row) = self.lib.playlists.iter().position(|p| p.id == id) else {
                        return vec![];
                    };
                    if x >= 404 && !grid {
                        let ids = self.playlist_ids(row);
                        return self.start_play_list(ids, 0);
                    }
                    self.open_playlist(row);
                    return vec![];
                }
                if x >= 404 && !grid {
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
                if x >= 404 && !grid {
                    return self.start_play_action(Action::ShuffleArtist(row));
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

    /// Open the artist page for a NAME, as a tag spells it. False when the library has no such
    /// artist — a tag can say anything, and the library is the authority on what exists.
    ///
    /// Case-insensitive, because tags are not consistent about it and a link that fails on
    /// capitalisation is worse than no link.
    fn open_artist_named(&mut self, name: &str) -> bool {
        let Some(i) = self
            .lib
            .artists
            .iter()
            .position(|a| a.name.eq_ignore_ascii_case(name))
        else {
            return false;
        };
        self.open_artist(i);
        true
    }

    /// Open the album page for a NAME. Album titles collide far more often than artist names
    /// ("Greatest Hits"), so this prefers the one belonging to the track's own artist and only
    /// falls back to the first match by title.
    fn open_album_named(&mut self, name: &str) -> bool {
        let artist = self
            .track_info
            .iter()
            .find(|(l, _)| l == "Album artist" || l == "Artist")
            .map(|(_, v)| v.clone())
            .unwrap_or_default();
        // Index copied out before the mutation below — `albums_flat` borrows `self.lib`.
        let idx = {
            let flat = self.lib.albums_flat();
            flat.iter()
                .position(|a| {
                    a.name.eq_ignore_ascii_case(name) && a.artist.eq_ignore_ascii_case(&artist)
                })
                .or_else(|| flat.iter().position(|a| a.name.eq_ignore_ascii_case(name)))
        };
        let Some(idx) = idx else { return false };
        self.album_view = idx;
        self.album_track_idx = 0;
        self.album_scroll_px = 0;
        self.fling_v = 0.0;
        self.push(Screen::Album);
        true
    }

    /// Push the artist page for `lib.artists[idx]`, from its top.
    fn open_artist(&mut self, idx: usize) {
        self.artist_view = idx;
        self.artist_track_idx = 0;
        self.artist_songs_open = false;
        self.artist_scroll_px = 0;
        self.fling_v = 0.0;
        self.push(Screen::Artist);
    }

    /// The open artist's page, resolved against the library. Borrows `self.lib`, so callers that
    /// need to mutate must copy what they want out first.
    fn artist_page(&self) -> Option<crate::library::ArtistPage<'_>> {
        self.lib
            .artists
            .get(self.artist_view)
            .map(|a| library::artist_page(&self.lib, &a.name, self.artist_songs_open))
    }

    /// The open artist page's tracks, in page order, as object ids.
    fn artist_list_ids(&self) -> Vec<i64> {
        self.artist_page()
            .map(|p| p.tracks.iter().map(|t| t.song.object_id).collect())
            .unwrap_or_default()
    }

    /// A tap on the artist page: an album row drills in, a track row plays.
    fn tap_artist(&mut self, x: i32, y: i32) -> Vec<Action> {
        let albums = self.artist_page().map_or(0, |p| p.albums.len());
        if library::hit_artist_shuffle_band(x, y, albums) {
            return self.start_play_action(Action::ShuffleArtist(self.artist_view));
        }
        if library::hit_artist_random_album(x, y, albums) {
            return self.play_random_album();
        }
        // The add button: every track on the page, in page order, to the end of Up Next.
        if library::hit_artist_add(x, y, albums) {
            let Some((name, rows)) = self.artist_page().map(|p| {
                let rows: Vec<SongRow> = p.tracks.iter().map(|t| t.song.clone()).collect();
                (p.name.to_string(), rows)
            }) else {
                return vec![];
            };
            if rows.is_empty() {
                return vec![];
            }
            self.notify_format("Added to Up Next — {}", vec![("0", name)]);
            self.queue_anim_y = y;
            self.queue_anim_frames = QUEUE_ANIM_FRAMES;
            return self.insert_rows(rows, QueueAt::Later);
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
            // The artist's tracks play as the artist's list, from the tapped one.
            Some((library::ArtistHit::Track(i), Some(_))) => {
                self.artist_track_idx = i;
                let ids = self.artist_list_ids();
                self.start_play_list(ids, i)
            }
            // Folding the list away can leave the page scrolled past its new end.
            Some((library::ArtistHit::ToggleSongs, _)) => {
                self.artist_songs_open = !self.artist_songs_open;
                self.fling_v = 0.0;
                let max = self
                    .artist_page()
                    .map(|p| library::artist_max_scroll_px(&p))
                    .unwrap_or(0);
                self.artist_scroll_px = self.artist_scroll_px.clamp(0, max);
                vec![]
            }
            _ => vec![],
        }
    }

    /// The artist page's Random album: one of their albums, in order, from its first track (the
    /// shell's album context plays the rest), named in a toast so the button explains itself. Not
    /// the album already playing when there is another to choose.
    fn play_random_album(&mut self) -> Vec<Action> {
        let playing = self.playing_object_id();
        let albums: Vec<(String, Vec<i64>)> = self
            .artist_page()
            .map(|p| {
                p.albums
                    .iter()
                    .map(|(_, a)| {
                        let ids = a.track_list.iter().map(|s| s.object_id).collect();
                        (a.name.clone(), ids)
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut pick: Vec<&(String, Vec<i64>)> =
            albums.iter().filter(|(_, ids)| !ids.is_empty()).collect();
        if pick.len() > 1 {
            pick.retain(|(_, ids)| playing.is_none_or(|p| !ids.contains(&p)));
        }
        if pick.is_empty() {
            return vec![];
        }
        let (title, ids) = pick[self.rng.below(pick.len())];
        let (title, first) = (title.clone(), ids[0]);
        // Raw: already translated by `ui_format!`, and an album name is not a catalogue key.
        self.notify_raw(&crate::ui_format!("Random album · {}", title));
        self.start_play(first)
    }

    /// Push the playlist page for `lib.playlists[idx]`, from its top.
    fn open_playlist(&mut self, idx: usize) {
        self.playlist_view = idx;
        self.playlist_track_idx = 0;
        self.playlist_scroll_px = 0;
        self.fling_v = 0.0;
        self.push(Screen::Playlist);
    }

    // ── Ratings, plays, saved views and the playlist editor (redesign R4) ────────────────────────

    /// What the shuffle button deals, as the settings word.
    pub fn shuffle_by(&self) -> &'static str {
        self.shuffle_by.token()
    }

    pub fn set_shuffle_by(&mut self, word: &str) {
        self.shuffle_by = crate::shuffle::ShuffleBy::from_token(word);
    }

    /// `cinder_views.conf`, as it should be on disk now. The shell writes it when it changes.
    pub fn views_body(&self) -> String {
        crate::views::serialize(&self.views)
    }

    /// Load the saved views from the file's text (the shell reads it once the volume is there).
    pub fn set_views_body(&mut self, body: &str) {
        self.views = crate::views::parse(body);
        self.rebuild_smart(None);
    }

    /// One track's stats changed (a play was counted). The default value removes the entry.
    pub fn set_track_stat(&mut self, object_id: i64, st: crate::model::TrackStat) {
        if st == crate::model::TrackStat::default() {
            self.lib.stats.remove(&object_id);
        } else {
            self.lib.stats.insert(object_id, st);
        }
        if !self.views.is_empty() {
            self.rebuild_smart(None);
        }
    }

    /// Rate the PLAYING track `stars` (1..=5); the rating it already has clears it. The UI's copy
    /// changes at once, and the action has the shell write it down.
    fn rate_playing(&mut self, stars: u8) -> Vec<Action> {
        let Some(id) = self.playing_object_id() else {
            self.notify(crate::i18n::text("Nothing is playing"));
            return vec![];
        };
        let mut st = self.lib.stat(id);
        st.rating = if st.rating == stars { 0 } else { stars.min(5) };
        let rating = st.rating;
        self.set_track_stat(id, st);
        vec![Action::RateTrack(id, rating)]
    }

    /// Recompute the smart playlists and put them at the top of the playlist rows, then find the
    /// open playlist again by id (`open`, or whichever is open now).
    ///
    /// A smart playlist's members are copies of the matching song rows, resolved here rather than
    /// per frame — the same trade every album and playlist page makes. It runs when something a
    /// rule reads has changed: the library, the views, a rating, a counted play.
    fn rebuild_smart(&mut self, open: Option<i64>) {
        let open = open.or_else(|| self.lib.playlists.get(self.playlist_view).map(|p| p.id));
        self.lib.playlists.retain(|p| !p.smart);
        let now = self.clock_epoch;
        let rows: Vec<crate::model::PlaylistRow> = self
            .views
            .iter()
            .map(|v| {
                let track_list: Vec<SongRow> = v
                    .tracks(&self.lib, now)
                    .into_iter()
                    .map(|i| self.lib.songs[i].clone())
                    .collect();
                crate::model::PlaylistRow {
                    id: v.id(),
                    name: v.name.clone(),
                    tracks: track_list.len() as u32,
                    art: v.name.clone(),
                    cover_album_id: track_list.first().map_or(0, |s| s.album_id),
                    smart: true,
                    rules: v.summary(),
                    track_list,
                    ..Default::default()
                }
            })
            .collect();
        self.lib.playlists.splice(0..0, rows);
        self.playlist_view = open
            .and_then(|id| self.lib.playlists.iter().position(|p| p.id == id))
            .unwrap_or_else(|| {
                self.playlist_view
                    .min(self.lib.playlists.len().saturating_sub(1))
            });
        self.az_memo = None;
    }

    /// The members of `lib.playlists[idx]` as object ids, in order.
    fn playlist_ids(&self, idx: usize) -> Vec<i64> {
        self.lib
            .playlists
            .get(idx)
            .map(|p| p.track_list.iter().map(|s| s.object_id).collect())
            .unwrap_or_default()
    }

    /// Open the editor on the playlist that is open. Only for one Cinder can write.
    fn open_playlist_edit(&mut self) {
        let Some(p) = self.playlist_row().filter(|p| p.user && !p.smart) else {
            return;
        };
        self.pl_edit = Some(PlEdit {
            id: p.id,
            name: p.name.clone(),
            rows: p.track_list.clone(),
            order: (0..p.track_list.len()).collect(),
            undo: Vec::new(),
            scroll_px: 0,
            glued: (0..p.track_list.len())
                .map(|i| p.glued.get(i).copied().unwrap_or(false))
                .collect(),
            glued0: (0..p.track_list.len())
                .map(|i| i > 0 && p.glued.get(i).copied().unwrap_or(false))
                .collect(),
        });
        self.fling_v = 0.0;
        self.push(Screen::PlaylistEdit);
    }

    fn tap_playlist_edit(&mut self, x: i32, y: i32) -> Vec<Action> {
        use crate::playlist_edit as pe;
        let Some(e) = self.pl_edit.as_mut() else {
            return vec![];
        };
        if crate::chrome::header_action_hit(x, y) {
            // DONE. An unchanged list writes nothing.
            let changed = e.changed();
            let id = e.id;
            let ids: Vec<i64> = e
                .order
                .iter()
                .filter_map(|&i| e.rows.get(i))
                .map(|s| s.object_id)
                .collect();
            let rows: Vec<SongRow> = e
                .order
                .iter()
                .filter_map(|&i| e.rows.get(i).cloned())
                .collect();
            let removed = e.rows.len() - rows.len();
            let glued = e.glue_in_order();
            self.pl_edit = None;
            self.pop();
            if !changed {
                return vec![];
            }
            // Show the result at once; the shell's refresh after the write replaces it with the
            // same thing read back from the file.
            if let Some(p) = self.lib.playlists.iter_mut().find(|p| p.id == id) {
                p.tracks = p.tracks.saturating_sub(removed as u32);
                p.track_list = rows;
                p.glued = glued.clone();
                p.edited = true;
            }
            self.playlist_track_idx = 0;
            self.playlist_scroll_px = 0;
            self.pl_edit_done = Some((id, ids, glued));
            self.notify(crate::i18n::text("Playlist saved"));
            return vec![Action::PlaylistSaveEdit(id)];
        }
        if pe::hit_name(y) {
            let id = e.id;
            self.open_keyboard(KbPurpose::Rename(id));
            return vec![];
        }
        if pe::hit_cover(y) {
            self.open_cover_pick();
            return vec![];
        }
        if !e.undo.is_empty() && pe::hit_undo(x, y) {
            if let Some(prev) = e.undo.pop() {
                e.order = prev;
            }
            return vec![];
        }
        // The middle of a row: glue it to the row above, or unglue it (plan 1c). Not the top row.
        if pe::hit_glue(x) {
            if let Some(slot) = pe::row_at(e.order.len(), e.scroll_px, y).filter(|&s| s > 0) {
                let i = e.order[slot];
                if let Some(g) = e.glued.get_mut(i) {
                    *g = !*g;
                }
                return vec![];
            }
        }
        if let Some(row) = pe::hit_remove(e.order.len(), e.scroll_px, x, y) {
            // One tap, not the page's two: UNDO is right there in the label, and nothing is
            // written until DONE.
            e.remove_row(row);
            e.scroll_px = e.scroll_px.clamp(0, pe::max_scroll(e.order.len()));
        }
        vec![]
    }

    /// The order the playlist editor's DONE produced: `(playlist id, member object ids in the new
    /// order)`. Taken by the shell when it carries out `Action::PlaylistSaveEdit`.
    pub fn take_playlist_edit(&mut self) -> Option<(i64, Vec<i64>, Vec<bool>)> {
        self.pl_edit_done.take()
    }

    /// Open the saved-view editor: on view `at`, or on a new one.
    fn open_view_edit(&mut self, at: Option<usize>) {
        if at.is_none() && self.views.len() >= crate::views::MAX_VIEWS {
            self.notify(crate::i18n::text("No room for another smart playlist"));
            return;
        }
        let draft = at
            .and_then(|i| self.views.get(i).cloned())
            .unwrap_or_default();
        self.view_edit = Some((at, draft));
        self.push(Screen::ViewEdit);
    }

    fn tap_view_edit(&mut self, x: i32, y: i32) -> Vec<Action> {
        use crate::view_edit as ve;
        let Some((at, draft)) = self.view_edit.as_mut() else {
            return vec![];
        };
        if crate::chrome::header_action_hit(x, y) {
            // SAVE. An unnamed view gets a name rather than a refusal, and a name another view
            // has is made unique, so every smart playlist keeps an id of its own.
            let at = *at;
            let mut v = draft.clone();
            v.name = crate::views::unique_name(&self.views, &crate::views::clean_name(&v.name), at);
            let id = v.id();
            // Was the page of the view being edited open underneath? Then it should still be.
            let was_open = at
                .and_then(|i| self.views.get(i))
                .map(|old| old.id())
                .filter(|old| {
                    self.lib
                        .playlists
                        .get(self.playlist_view)
                        .is_some_and(|p| p.id == *old)
                });
            match at {
                Some(i) if i < self.views.len() => self.views[i] = v,
                _ => self.views.push(v),
            }
            self.view_edit = None;
            self.pop();
            self.rebuild_smart(was_open.map(|_| id));
            // A new one lands inside itself, the way a new playlist does.
            if at.is_none() {
                self.open_playlist_by_id(id);
            }
            self.notify(crate::i18n::text("Smart playlist saved"));
            return vec![];
        }
        if ve::hit_name(y) {
            self.open_keyboard(KbPurpose::ViewName);
            return vec![];
        }
        if let Some((section, chip)) = ve::chip_at(x, y) {
            section.select(draft, chip);
            return vec![];
        }
        if at.is_some() && ve::hit_delete(y) {
            self.confirm = Some(crate::confirm::Ask::DeleteView);
        }
        vec![]
    }

    /// The Delete card's Confirm: drop the view being edited, and leave both the editor and the
    /// smart playlist's page, which is about to stop existing.
    fn delete_edited_view(&mut self) -> Vec<Action> {
        let Some((Some(at), _)) = self.view_edit.take() else {
            return vec![];
        };
        if at >= self.views.len() {
            return vec![];
        }
        let id = self.views.remove(at).id();
        if self.current() == Screen::ViewEdit {
            self.pop();
        }
        let on_its_page = self.current() == Screen::Playlist
            && self
                .lib
                .playlists
                .get(self.playlist_view)
                .is_some_and(|p| p.id == id);
        if on_its_page {
            self.pop();
        }
        self.rebuild_smart(None);
        self.notify(crate::i18n::text("Smart playlist deleted"));
        vec![]
    }

    /// The open playlist, or None if the library changed under us.
    fn playlist_row(&self) -> Option<&crate::model::PlaylistRow> {
        self.lib.playlists.get(self.playlist_view)
    }

    /// Cinder's own playlists — the only ones anything can be added to.
    fn user_playlists(&self) -> Vec<(i64, String, u32)> {
        self.lib
            .playlists
            .iter()
            .filter(|p| p.user)
            .map(|p| (p.id, p.name.clone(), p.tracks))
            .collect()
    }

    /// Open the keyboard for `purpose`, seeded with the name it is editing (empty for a new one).
    fn open_keyboard(&mut self, purpose: KbPurpose) {
        self.kb_purpose = purpose;
        self.kb_shift = false;
        self.kb_page = 0;
        self.kb_text = match purpose {
            // Editing a search starts from what is already typed.
            KbPurpose::TrackSearch => self.track_pick_query.clone(),
            KbPurpose::LibrarySearch => self.search_query.clone(),
            // A name to accept rather than to type: the album when the list is one album, else
            // "Up Next". Typing on this keyboard is the slow part.
            KbPurpose::SaveQueue => self.queue_save_name(),
            // The folder's name: a playlist made from one folder is usually named after it.
            KbPurpose::NewPlaylistWithPicked => self.pick_many_name.clone(),
            KbPurpose::ViewName => self
                .view_edit
                .as_ref()
                .map(|(_, d)| d.name.clone())
                .unwrap_or_default(),
            // A name to accept or edit: "Preset N" for a new one, the current name for a rename.
            KbPurpose::PresetName(None) => crate::presets::name(&[], self.presets.len()),
            KbPurpose::PresetName(Some(i)) => crate::presets::name(&self.preset_names, i),
            // A new search starts from the last one: the usual change is a word.
            KbPurpose::RadioFind => self.radio_query.clone(),
            KbPurpose::MediaSearch => self.media_query.clone(),
            // Renaming starts from the current name: the common edit is a word, not a retype.
            KbPurpose::Rename(id) => self
                .lib
                .playlists
                .iter()
                .find(|p| p.id == id)
                .map(|p| p.name.clone())
                .unwrap_or_default(),
            _ => String::new(),
        };
        self.push(Screen::Keyboard);
    }

    /// The text the keyboard is holding. The shell reads this when it carries out a
    /// `PlaylistCreate` / `PlaylistRename`, because `Action` is `Copy` and cannot carry a String.
    pub fn text_input(&self) -> &str {
        self.kb_text.trim()
    }

    /// Replace just the playlist rows. The shell calls this after it writes the store, instead of
    /// rebuilding the whole library: a rebuild is one query per album plus one per playlist, and
    /// it would also reset the scroll position of the screen the user is looking at.
    pub fn set_playlists(&mut self, rows: Vec<crate::model::PlaylistRow>) {
        // Follow the OPEN playlist by id, not by row number. The merged list is sorted by name, so
        // a rename moves it — and an index kept across that would leave the page showing whichever
        // playlist slid into the old slot, under the old name's edits.
        let open = self.lib.playlists.get(self.playlist_view).map(|p| p.id);
        self.lib.playlists = rows;
        // The shell's rows are the stored playlists only; the smart ones are ours to put back on
        // top, and the open page is then found again by id among all of them.
        self.rebuild_smart(open);
    }

    /// Put the page for playlist `id` on screen. Used by the shell right after it creates one, so
    /// naming a playlist lands you inside it rather than back at a list you have to search.
    pub fn open_playlist_by_id(&mut self, id: i64) {
        if let Some(index) = self.lib.playlists.iter().position(|p| p.id == id) {
            if self.current() == Screen::Playlist {
                self.playlist_view = index;
                self.playlist_track_idx = 0;
                self.playlist_scroll_px = 0;
            } else {
                self.open_playlist(index);
            }
        }
    }

    /// The object id of the track actually PLAYING: the sequence row at `context_idx`.
    ///
    /// Not `current_song_object_id()`: that one resolves the highlighted LIBRARY row, which is
    /// whatever list the user last scrolled and has nothing to do with what is coming out of the
    /// headphones.
    fn playing_object_id(&self) -> Option<i64> {
        self.context.get(self.context_idx).map(|s| s.object_id)
    }

    /// The album page's row marked as playing: the one PLAYING. It was the last row tapped there,
    /// or row 0 on arrival, so every album opened read as playing its first track (seen on the
    /// player 2026-10-24). A CUE album is one file, and which of its tracks is under the playhead
    /// is the shell's to know: there, the row tapped stands in.
    fn album_playing_row(&self, al: &crate::model::AlbumRow) -> Option<usize> {
        let id = self.playing_object_id()?;
        if al.cue.is_empty() {
            al.track_list.iter().position(|s| s.object_id == id)
        } else {
            al.track_list
                .iter()
                .any(|s| s.object_id == id)
                .then_some(self.album_track_idx)
        }
    }

    /// Open "Add to playlist" for one track.
    pub fn open_playlist_pick(&mut self, object_id: i64) {
        self.pick_track = object_id;
        self.pick_many.clear();
        self.pick_scroll_px = 0;
        self.fling_v = 0.0;
        self.push(Screen::PlaylistPick);
    }

    /// Open "Add to playlist" for whole folders: each one's songs and every subfolder's, in browse
    /// order, folders in the order they were ticked (#56).
    fn open_playlist_pick_folders(&mut self, dirs: &[usize]) {
        let mut ids = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for &dir in dirs {
            let mut stack = vec![dir];
            while let Some(d) = stack.pop() {
                let Some(f) = self.lib.folders.get(d) else {
                    continue;
                };
                ids.extend(
                    f.tracks
                        .iter()
                        .map(|t| t.object_id)
                        .filter(|id| seen.insert(*id)),
                );
                stack.extend(f.subdirs.iter().rev());
            }
        }
        if ids.is_empty() {
            self.notify(crate::i18n::text("No songs in this folder"));
            return;
        }
        self.pick_many_name = match dirs {
            [one] => self
                .lib
                .folders
                .get(*one)
                .map(|f| f.name.clone())
                .unwrap_or_default(),
            _ => crate::ui_format!("{n} folders", n = dirs.len()),
        };
        self.pick_many = ids;
        self.pick_track = 0;
        self.pick_scroll_px = 0;
        self.fling_v = 0.0;
        self.push(Screen::PlaylistPick);
    }

    /// The folder's songs for `PlaylistAddPicked` / `PlaylistCreateWithPicked`, once.
    pub fn take_pick_many(&mut self) -> Vec<i64> {
        std::mem::take(&mut self.pick_many)
    }

    /// Open "Add tracks" for the playlist that is open.
    fn open_track_pick(&mut self) {
        // A fresh visit starts unfiltered — the previous search was about a different errand.
        self.track_pick_query.clear();
        self.track_pick_rebuild();
        self.track_pick_scroll_px = 0;
        self.fling_v = 0.0;
        self.push(Screen::TrackPick);
    }

    /// Rebuild the "Add tracks" list from the current query.
    ///
    /// The reference device holds 3,462 tracks. Scrolling to one of them at 64 px a row is about
    /// 220,000 px of travel, so an unfiltered flat list is not a way of FINDING anything — it is
    /// only a way of browsing. The filter matches title, artist and album, because which of the
    /// three you remember is not predictable.
    ///
    /// Indices stay ABSOLUTE into `lib.songs`, exactly as `song_order` returns them, so the row
    /// resolution below is unchanged whether or not a filter is on.
    fn track_pick_rebuild(&mut self) {
        let order = self.songs_matching(&self.track_pick_query);
        self.track_pick_order = order;
        self.track_pick_scroll_px = 0;
    }

    /// Songs whose title, artist or album name contains `query`, case-insensitively, as absolute
    /// indices into `lib.songs` in title order. An empty query is every song. Shared by "Add tracks"
    /// and library search, so the two can never disagree about what matches.
    fn songs_matching(&self, query: &str) -> Vec<usize> {
        let all = library::song_order(&self.lib, 0);
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            all
        } else {
            // A SongRow carries `album_id`, not the album name, so the names are resolved once
            // here rather than per row — the alternative is a linear album scan per track, which
            // on 3,462 tracks against 343 albums is over a million comparisons for one search.
            let albums: std::collections::HashMap<i64, String> = self
                .lib
                .albums_flat()
                .iter()
                .map(|a| (a.album_id, a.name.to_lowercase()))
                .collect();
            all.into_iter()
                .filter(|i| {
                    self.lib.songs.get(*i).is_some_and(|s| {
                        s.title.to_lowercase().contains(&q)
                            || s.artist.to_lowercase().contains(&q)
                            || albums.get(&s.album_id).is_some_and(|a| a.contains(&q))
                    })
                })
                .collect()
        }
    }

    fn pick_max_scroll(&self) -> i32 {
        let rows = self.user_playlists().len() + 1;
        (crate::playlist_pick::content_h(rows)
            - (crate::playlist_pick::BOTTOM - crate::playlist_pick::TOP))
            .max(0)
    }

    fn track_pick_max_scroll(&self) -> i32 {
        crate::playlist_pick::tracks_max_scroll(self.track_pick_order.len())
    }

    /// The songs the track picker is showing, in its fixed order.
    fn track_pick_songs(&self) -> Vec<&crate::model::SongRow> {
        self.track_pick_order
            .iter()
            .filter_map(|i| self.lib.songs.get(*i))
            .collect()
    }

    /// A tap on the playlist page: the band's left half plays it in order and its right half
    /// shuffles it; a track row plays the WHOLE playlist starting there.
    fn tap_playlist(&mut self, x: i32, y: i32) -> Vec<Action> {
        let Some((id, user, smart)) = self.playlist_row().map(|p| (p.id, p.user, p.smart)) else {
            return vec![];
        };
        // EDIT, in the header's right slot: the member editor for a list Cinder owns, the rules
        // for a smart one. Sony's draw no EDIT and `hit_playlist_edit` refuses them.
        if self
            .playlist_row()
            .is_some_and(|p| library::hit_playlist_edit(p, x, y))
        {
            if smart {
                let at = self.views.iter().position(|v| v.id() == id);
                if at.is_some() {
                    self.open_view_edit(at);
                }
            } else {
                self.open_playlist_edit();
            }
            return vec![];
        }
        if smart {
            // A smart playlist has no file and no database row: it plays as the list on screen,
            // through the same channel the Songs tab and the artist page use.
            let ids = self.playlist_ids(self.playlist_view);
            if library::hit_playlist_play_band(x, y) {
                self.playlist_track_idx = 0;
                return self.start_play_list(ids, 0);
            }
            if library::hit_playlist_shuffle_band(x, y) {
                if ids.is_empty() {
                    return vec![];
                }
                self.play_list = ids;
                // The view's own mode, or the setting when it has none (2026-10-04).
                self.list_shuffle_by = self
                    .views
                    .iter()
                    .find(|v| v.id() == id)
                    .and_then(|v| v.shuffle)
                    .unwrap_or(self.shuffle_by);
                return self.start_play_action(Action::ShuffleList);
            }
            let hit = self
                .playlist_row()
                .and_then(|p| library::playlist_hit_track(p, self.playlist_scroll_px, y));
            return match hit {
                Some(i) => {
                    self.playlist_track_idx = i;
                    self.start_play_list(ids, i)
                }
                None => vec![],
            };
        }
        if library::hit_playlist_play_band(x, y) {
            self.playlist_track_idx = 0;
            return self.start_play_action(Action::PlayPlaylist(id));
        }
        if library::hit_playlist_shuffle_band(x, y) {
            return self.start_play_action(Action::ShufflePlaylist(id));
        }
        if user {
            // THE COVER, which is a button on a user playlist: it opens the cover picker.
            if self
                .playlist_row()
                .is_some_and(|p| library::hit_playlist_cover(p, x, y))
            {
                self.open_cover_pick();
                return vec![];
            }
            // The edit bar, above the list.
            if let Some(action) = self
                .playlist_row()
                .and_then(|p| library::hit_playlist_action(p, x, y))
            {
                match action {
                    0 => self.open_track_pick(),
                    1 => self.open_keyboard(KbPurpose::Rename(id)),
                    _ => self.confirm = Some(crate::confirm::Ask::DeletePlaylist),
                }
                return vec![];
            }
        }
        // Copy the object id out before mutating — `playlist_row` borrows `self.lib`.
        let hit = self.playlist_row().and_then(|p| {
            library::playlist_hit_track(p, self.playlist_scroll_px, y)
                .and_then(|i| p.track_list.get(i).map(|s| (i, s.object_id)))
        });
        match hit {
            // The PLAYLIST is the context, not the tapped track's album — see
            // `Action::PlayPlaylistAt`. `oid` is resolved only to prove the row is real.
            Some((i, _oid)) => {
                self.playlist_track_idx = i;
                self.start_play_action(Action::PlayPlaylistAt(id, i as u32))
            }
            None => vec![],
        }
    }

    /// Open the cover picker on the playlist that is open (one Cinder can write).
    fn open_cover_pick(&mut self) {
        if self.playlist_row().is_some_and(|p| p.user && !p.smart) {
            self.cover_scroll_px = 0;
            self.fling_v = 0.0;
            self.push(Screen::PlaylistCover);
        }
    }

    fn cover_choices(&self) -> Vec<crate::playlist_cover::Choice> {
        self.playlist_row()
            .map(|p| crate::playlist_cover::choices(&self.lib, p))
            .unwrap_or_default()
    }

    /// A choice in the cover picker: saved at once, named in a toast, and back to where it opened.
    fn tap_cover_pick(&mut self, y: i32) -> Vec<Action> {
        use crate::playlist_cover::Choice;
        let choices = self.cover_choices();
        let Some(i) = crate::playlist_cover::hit_row(choices.len(), self.cover_scroll_px, y) else {
            return vec![];
        };
        let Some(id) = self.playlist_row().map(|p| p.id) else {
            return vec![];
        };
        self.pop();
        match &choices[i] {
            Choice::Auto => {
                self.notify(crate::i18n::text("Cover: automatic"));
                vec![Action::PlaylistSetCover(id, 0)]
            }
            Choice::Album {
                object_id, name, ..
            } => {
                self.notify_format("Cover: {label}", vec![("label", name.clone())]);
                vec![Action::PlaylistSetCover(id, *object_id)]
            }
            Choice::Picture(p) => {
                let name = self.lib.cover_pictures[*p].name.clone();
                self.notify_format("Cover: {label}", vec![("label", name)]);
                vec![Action::PlaylistSetPicture(id, *p as u32)]
            }
        }
    }

    /// What the open playlist's cover is, in words, for the editor's Cover row.
    fn cover_label(&self) -> String {
        let Some(p) = self.playlist_row() else {
            return String::new();
        };
        if let Some(pic) = self
            .lib
            .playlist_pictures
            .get(&p.id)
            .and_then(|&i| self.lib.cover_pictures.get(i))
        {
            return pic.name.clone();
        }
        match p
            .track_list
            .iter()
            .find(|t| p.cover_custom && t.album_id == p.cover_album_id)
        {
            Some(t) => t.art.clone(),
            None => crate::i18n::text("Automatic").to_string(),
        }
    }

    /// Anything that has to happen after a key changes the buffer, whichever route the key came
    /// in by — so `type_for_test` cannot diverge from what a finger does.
    ///
    /// A SEARCH FILTERS AS YOU TYPE. Nothing has to be "submitted": the count in the header
    /// updates on every key, so you can see the search working before committing to it, leaving by
    /// Back keeps what you typed, and the feature does not hang on the user finding the DONE key.
    fn kb_after_key(&mut self) {
        if matches!(self.kb_purpose, KbPurpose::TrackSearch) {
            self.track_pick_query = self.kb_text.clone();
            self.track_pick_rebuild();
        }
        if matches!(self.kb_purpose, KbPurpose::LibrarySearch) {
            self.search_query = self.kb_text.clone();
            self.search_rebuild();
        }
    }

    /// A tap on the keyboard. Only Done leaves an action behind; everything else edits the buffer.
    fn tap_keyboard(&mut self, x: i32, y: i32) -> Vec<Action> {
        let Some(key) = crate::keyboard::hit(self.kb_page, x, y) else {
            return vec![];
        };
        let commit = crate::keyboard::apply(
            key,
            &mut self.kb_text,
            &mut self.kb_shift,
            &mut self.kb_page,
        );
        self.kb_after_key();
        if !commit {
            return vec![];
        }
        let purpose = self.kb_purpose;
        // Leave the keyboard BEFORE the action lands: the shell rebuilds the playlist rows while
        // carrying it out, and it may want to push the new playlist's page on top of where we
        // came from — not on top of a keyboard the user has finished with.
        self.pop();
        // Naming a playlist FROM the "add to playlist" picker answers the picker's question too,
        // so the picker goes with it. Otherwise Back from the new playlist's page would drop the
        // user into a chooser they have already finished with.
        if matches!(
            purpose,
            KbPurpose::NewPlaylistWith(_) | KbPurpose::NewPlaylistWithPicked
        ) && self.current() == Screen::PlaylistPick
        {
            self.pop();
        }
        match purpose {
            // Already applied on every keystroke above; DONE just closes the keyboard. Screen
            // state, not an Action — nothing outside this app needs to know about a filter.
            KbPurpose::TrackSearch | KbPurpose::LibrarySearch => vec![],
            KbPurpose::NewPlaylist => vec![Action::PlaylistCreate],
            KbPurpose::NewPlaylistWith(object_id) => vec![Action::PlaylistCreateWith(object_id)],
            KbPurpose::NewPlaylistWithPicked => vec![Action::PlaylistCreateWithPicked],
            KbPurpose::Rename(id) => {
                // The editor's Name row shows the new name at once; the shell renames the file.
                if let Some(e) = self.pl_edit.as_mut().filter(|e| e.id == id) {
                    e.name = self.kb_text.trim().to_string();
                }
                vec![Action::PlaylistRename(id)]
            }
            KbPurpose::SaveQueue => vec![Action::PlaylistCreateFromQueue],
            KbPurpose::ViewName => {
                let name = crate::views::clean_name(&self.kb_text);
                if let Some((_, draft)) = self.view_edit.as_mut() {
                    draft.name = name;
                }
                vec![]
            }
            KbPurpose::PresetName(at) => self.preset_named(at),
            KbPurpose::MediaSearch => {
                let q = self.kb_text.trim().to_string();
                if q.is_empty() || self.media_levels.is_empty() {
                    return vec![];
                }
                self.media_query = q;
                self.media_busy = true;
                self.media_note.clear();
                vec![Action::MediaFind {
                    depth: self.media_levels.len() - 1,
                }]
            }
            KbPurpose::RadioFind => {
                let q = self.kb_text.trim().to_string();
                if q.is_empty() {
                    return vec![];
                }
                self.radio_query = q;
                self.radio_found.clear();
                self.radio_find_note.clear();
                self.radio_finding = true;
                self.push(Screen::RadioFind);
                vec![Action::RadioFind]
            }
            KbPurpose::WifiPassword => match self.wifi_join_target.take() {
                Some(ssid) => {
                    self.wifi_join = Some((ssid, std::mem::take(&mut self.kb_text)));
                    self.wifi_status.state = crate::wifi::State::Busy;
                    self.wifi_status.note.clear();
                    crate::wifi::note_state(crate::wifi::State::Busy);
                    vec![Action::WifiJoin]
                }
                None => vec![],
            },
        }
    }

    /// A tap on Settings ▸ Wi-Fi.
    fn tap_wifi(&mut self, x: i32, y: i32) -> Vec<Action> {
        use crate::wifi::{Hit, State};
        let on = !matches!(self.wifi_status.state, State::Off);
        match crate::wifi::hit(x, y, on, self.update_offered()) {
            Hit::Update => {
                if self.update_running {
                    return vec![];
                }
                if self.too_flat_to_update() {
                    // The install rewrites the system partition, and nothing below Cinder checks
                    // the battery: Sony's own updater on the player does not either.
                    self.notify(crate::i18n::text(
                        "Charge first: updating needs 30% or a cable",
                    ));
                } else {
                    self.confirm = Some(crate::confirm::Ask::WifiUpdate);
                }
                vec![]
            }
            Hit::Toggle => {
                if matches!(self.wifi_status.state, State::Busy) {
                    return vec![];
                }
                self.wifi_status.state = State::Busy;
                self.wifi_status.note.clear();
                crate::wifi::note_state(State::Busy);
                if on {
                    self.wifi_networks.clear();
                }
                vec![Action::WifiPower(!on)]
            }
            Hit::Networks => {
                self.push(Screen::WifiNetworks);
                // Opening the list with nothing in it is a request to look.
                if !matches!(self.wifi_status.state, State::Busy)
                    && self.wifi_networks.is_empty()
                    && !self.wifi_scanning
                {
                    self.wifi_scanning = true;
                    return vec![Action::WifiScan];
                }
                vec![]
            }
            Hit::Radio => {
                self.push(Screen::NetRadio);
                self.radio_scroll_px = 0;
                vec![Action::RadioOpen]
            }
            Hit::Speakers => {
                self.push(Screen::WifiSpeakers);
                // Opening the page is a request to look (speakers come and go).
                if self.speakers_scanning {
                    return vec![];
                }
                self.speakers_scanning = true;
                vec![Action::SpeakersScan]
            }
            Hit::Media => {
                self.push(Screen::MediaServers);
                self.media_levels.clear();
                self.media_note.clear();
                self.media_busy = true;
                vec![Action::MediaSearch]
            }
            Hit::Remote => {
                self.push(Screen::WifiRemote);
                vec![]
            }
            Hit::AirPlay => {
                self.toggle_opt(Opt::WifiAirplay);
                if !self.opt(Opt::WifiAirplay) {
                    self.airplay_note.clear();
                }
                vec![Action::WifiAirPlay]
            }
            Hit::Options => {
                self.push(Screen::WifiOptions);
                vec![]
            }
            _ => vec![],
        }
    }

    /// A tap on Settings ▸ Wi-Fi ▸ Options.
    fn tap_wifi_options(&mut self, _x: i32, y: i32) -> Vec<Action> {
        use crate::wifi::{Hit, State};
        let connected = matches!(self.wifi_status.state, State::Connected);
        match crate::wifi::hit_options(y) {
            Hit::AutoStart => {
                self.toggle_opt(Opt::WifiAutostart);
                vec![]
            }
            Hit::Clock => {
                self.toggle_opt(Opt::WifiClock);
                if self.opt(Opt::WifiClock) && connected {
                    return vec![Action::WifiTimeSync];
                }
                vec![]
            }
            Hit::ListenBrainz => {
                self.toggle_opt(Opt::WifiLb);
                if !self.opt(Opt::WifiLb) {
                    self.wifi_lb_note.clear();
                } else if connected {
                    return vec![Action::WifiListenBrainz];
                }
                vec![]
            }
            Hit::Lyrics => {
                self.toggle_opt(Opt::WifiLyrics);
                vec![]
            }
            Hit::Art => {
                self.toggle_opt(Opt::WifiArt);
                vec![]
            }
            Hit::PlayTo => {
                self.toggle_opt(Opt::WifiPlayTo);
                self.play_to_note.clear();
                vec![]
            }
            Hit::Drive => {
                self.toggle_opt(Opt::WifiDrive);
                self.drive_note.clear();
                vec![]
            }
            Hit::Updates => {
                self.toggle_opt(Opt::WifiUpdates);
                if !self.opt(Opt::WifiUpdates) {
                    self.wifi_update_note.clear();
                } else if connected {
                    return vec![Action::WifiUpdateCheck];
                }
                vec![]
            }
            _ => vec![],
        }
    }

    /// `http://<ip>:<port>` while the phone remote is on.
    fn remote_address(&self) -> String {
        format!("http://{}:{}", self.wifi_status.ip, self.remote_port)
    }
    fn remote_row_note(&self) -> String {
        if self.remote_on {
            self.remote_address()
        } else if self.opt(Opt::WifiRemote) {
            crate::i18n::text("On").to_string()
        } else {
            crate::i18n::text("Off").to_string()
        }
    }
    /// A tap on Settings ▸ Wi-Fi ▸ Phone remote: the switch (saved; the server runs whenever Wi-Fi
    /// is connected), and Forget phones while any are paired.
    fn tap_remote(&mut self, _x: i32, y: i32) -> Vec<Action> {
        if self.remote_phones > 0 && crate::wifi::hit_remote_forget(y) {
            self.remote_phones = 0;
            self.notify(crate::i18n::text("Phones forgotten"));
            return vec![Action::RemoteForget];
        }
        if !crate::wifi::hit_remote(y) {
            return vec![];
        }
        self.toggle_opt(Opt::WifiRemote);
        let on = self.opt(Opt::WifiRemote);
        if !on {
            self.remote_on = false;
            self.remote_code.clear();
        }
        vec![Action::Remote(on)]
    }
    /// cinder-ffi: how many phones open the remote without the code.
    pub fn set_remote_phones(&mut self, n: usize) {
        self.remote_phones = n;
    }
    pub fn remote_phones(&self) -> usize {
        self.remote_phones
    }
    /// The phone remote's Play: these tracks, from row `at`, in place of what was playing. No
    /// replace-or-keep question: it would be asked on the player, away from the phone, and the
    /// page has Play next and Add to queue for keeping.
    pub fn remote_play(&mut self, ids: Vec<i64>, at: usize) -> Vec<Action> {
        if at >= ids.len() {
            return vec![];
        }
        self.play_list = ids;
        vec![Action::PlayListAt(at)]
    }
    /// The phone remote's Play next (`next`) and Add to queue: library tracks by id.
    pub fn remote_queue(&mut self, ids: &[i64], next: bool) -> Vec<Action> {
        let rows: Vec<SongRow> = ids
            .iter()
            .filter_map(|id| self.lib.songs.iter().find(|s| s.object_id == *id).cloned())
            .collect();
        let Some(first) = rows.first() else {
            return vec![];
        };
        let name = if rows.len() == 1 {
            first.title.clone()
        } else {
            crate::ui_format!("{} songs", rows.len())
        };
        if next {
            self.notify_format("Playing next — {}", vec![("0", name)]);
        } else {
            self.notify_format("Added to Up Next — {}", vec![("0", name)]);
        }
        self.insert_rows(rows, if next { QueueAt::Next } else { QueueAt::Later })
    }
    /// cinder-ffi: the server is up with this code on this port (or did not start: `on` false).
    pub fn set_remote(&mut self, on: bool, code: &str, port: u16) {
        self.remote_on = on;
        self.remote_code = code.to_string();
        self.remote_port = port;
    }
    pub fn remote_on(&self) -> bool {
        self.remote_on
    }
    /// True once after the link took the remote down (cinder-ffi then stops the server).
    pub fn take_remote_dropped(&mut self) -> bool {
        std::mem::take(&mut self.remote_dropped)
    }

    /// A tap on Settings ▸ Wi-Fi ▸ Speakers.
    fn tap_speakers(&mut self, x: i32, y: i32) -> Vec<Action> {
        use crate::wifi::SpeakerHit;
        if !matches!(self.wifi_status.state, crate::wifi::State::Connected) {
            return vec![];
        }
        match crate::wifi::hit_speakers(x, y, self.speakers.len(), self.cast_speaker.is_some()) {
            SpeakerHit::Scan if !self.speakers_scanning => {
                self.speakers_scanning = true;
                vec![Action::SpeakersScan]
            }
            SpeakerHit::Here => {
                self.cast_speaker = None;
                self.cast_line.clear();
                vec![Action::SpeakerStop]
            }
            SpeakerHit::Speaker(i) => {
                let name = self.speakers[i].0.clone();
                if self.cast_speaker.as_deref() == Some(name.as_str()) {
                    return vec![];
                }
                self.cast_line = crate::ui_format!("CONNECTING · {}", name);
                self.cast_speaker = Some(name);
                vec![Action::SpeakerPlay(i)]
            }
            _ => vec![],
        }
    }

    /// What Internet radio ▸ FIND is looking for.
    pub fn radio_query(&self) -> &str {
        &self.radio_query
    }
    /// Set it without the keyboard (previews).
    pub fn set_radio_query(&mut self, q: &str) {
        self.radio_query = q.to_string();
    }
    /// The stations a FIND search found (and why there are none, if there are none).
    pub fn set_radio_found(&mut self, found: Vec<crate::netradio::Found>, note: String) {
        self.radio_found = found;
        self.radio_find_note = note;
        self.radio_finding = false;
    }
    /// Search result `i` (cinder-ffi adds it and plays it).
    pub fn radio_found(&self, i: usize) -> Option<&crate::netradio::Found> {
        self.radio_found.get(i)
    }
    /// Play station `i` of the list (just added from FIND): back to the station list, showing it
    /// connecting; the shell's `take_radio` then hands it over.
    pub fn play_radio_station(&mut self, i: usize) {
        if self.current() == Screen::RadioFind {
            self.pop();
        }
        self.radio_pick = Some(i);
        self.radio_status = crate::netradio::Status {
            state: crate::netradio::State::Connecting,
            station: Some(i),
            ..Default::default()
        };
    }

    /// The speakers a search found, (name, model) each, in the order to show them.
    pub fn set_speakers(&mut self, list: Vec<(String, String)>) {
        self.speakers = list;
        self.speakers_scanning = false;
    }
    /// The cast's state from cinder-ffi: the speaker playing (None = the player), and the strip.
    pub fn set_cast(&mut self, speaker: Option<String>, line: String) {
        self.cast_speaker = speaker;
        self.cast_line = line;
    }
    /// A cast is on: play/pause, skip and volume go to the speaker.
    pub fn casting(&self) -> bool {
        self.cast_speaker.is_some()
    }

    /// The Wi-Fi page's Internet radio row, second line.
    fn radio_row_note(&self) -> String {
        use crate::netradio::State;
        match self.radio_status.state {
            State::Off => match self.radio_stations.len() {
                0 => crate::i18n::text("Stations from cinder_radio.txt").to_string(),
                1 => crate::i18n::text("1 station").to_string(),
                n => crate::ui_format!("{} stations", n),
            },
            _ => crate::netradio::strip_line(&self.radio_status, &self.radio_stations),
        }
    }

    /// A tap on Settings ▸ Wi-Fi ▸ Internet radio: a station plays, the playing one stops, FIND
    /// searches for more (while connected).
    fn tap_radio(&mut self, x: i32, y: i32) -> Vec<Action> {
        use crate::netradio::State;
        if crate::netradio::hit_find(x, y, self.radio_scroll_px) {
            if matches!(self.wifi_status.state, crate::wifi::State::Connected) {
                self.open_keyboard(KbPurpose::RadioFind);
            }
            return vec![];
        }
        let Some(i) = crate::netradio::hit(x, y, self.radio_stations.len(), self.radio_scroll_px)
        else {
            return vec![];
        };
        if self.radio_status.station == Some(i) && !matches!(self.radio_status.state, State::Off) {
            self.radio_status.state = State::Off;
            self.radio_status.note.clear();
            return vec![Action::RadioStop];
        }
        self.radio_pick = Some(i);
        self.radio_status = crate::netradio::Status {
            state: State::Connecting,
            station: Some(i),
            ..Default::default()
        };
        vec![Action::RadioPlay]
    }

    /// cinder_radio.txt's text (or "" when there is none).
    pub fn set_radio_stations(&mut self, text: &str) {
        self.radio_stations = crate::netradio::parse_stations(text);
        self.radio_scroll_px = self
            .radio_scroll_px
            .min(crate::netradio::max_scroll(self.radio_stations.len()));
    }
    /// The shell's radio status (`netradio::parse_status`).
    pub fn set_radio_status(&mut self, text: &str) {
        self.radio_status = crate::netradio::parse_status(text);
    }

    /// Now Playing's two lines while sound comes from a live source (`live::*`, as the shell
    /// reports it), built from what that source's page already knows. `dlna_title` is the DLNA
    /// controller's own title for its link. The rules are `live::lines`.
    pub fn live_lines(&self, kind: u8, dlna_title: &str) -> (String, String) {
        let station = self
            .radio_status
            .station
            .and_then(|i| self.radio_stations.get(i))
            .map(|s| s.name.as_str());
        crate::live::lines(
            kind,
            &crate::live::Inputs {
                station,
                radio_title: &self.radio_status.title,
                dlna_title,
                airplay: &self.airplay_meta,
                fm_khz: self.fm_khz,
                rx_peer: &self.rx_peer,
                rx_codec: self.rx_codec,
                usb_dac_fmt: self.usb_dac_fmt,
            },
        )
    }
    /// What an AirPlay sender says is playing (shairport-sync's metadata): title, artist, album.
    /// Empty strings when it says nothing.
    pub fn set_airplay_meta(&mut self, title: &str, artist: &str, album: &str) {
        self.airplay_meta = crate::live::AirPlayMeta {
            title: title.to_string(),
            artist: artist.to_string(),
            album: album.to_string(),
        };
    }
    /// Pick station `i` as a tap on it would (dev command channel). False when there is none.
    pub fn pick_radio(&mut self, i: usize) -> bool {
        let ok = i < self.radio_stations.len();
        if ok {
            self.radio_pick = Some(i);
        }
        ok
    }
    /// The station the last `RadioPlay` asked for: its index and URL. Taken once.
    pub fn take_radio(&mut self) -> Option<(usize, String)> {
        let i = self.radio_pick.take()?;
        self.radio_stations.get(i).map(|s| (i, s.url.clone()))
    }

    /// A tap on Settings ▸ Wi-Fi ▸ Network.
    fn tap_wifi_list(&mut self, x: i32, y: i32) -> Vec<Action> {
        use crate::wifi::{ListHit, State};
        if matches!(self.wifi_status.state, State::Off | State::Busy) {
            return vec![];
        }
        match crate::wifi::hit_list(x, y, self.wifi_networks.len()) {
            ListHit::Scan if !self.wifi_scanning => {
                self.wifi_scanning = true;
                vec![Action::WifiScan]
            }
            ListHit::Wps if !self.wifi_wps => {
                if matches!(self.wifi_status.state, State::Off) {
                    self.notify(crate::i18n::text("Switch Wi-Fi on first"));
                    return vec![];
                }
                self.wifi_wps = true;
                self.wifi_status.state = State::Busy;
                crate::wifi::note_state(State::Busy);
                vec![Action::WifiWps]
            }
            ListHit::Network(i) => {
                let Some(n) = self.wifi_networks.get(i).cloned() else {
                    return vec![];
                };
                // The network it is on: the one thing to do with it is forget it.
                if matches!(self.wifi_status.state, State::Connected | State::Connecting)
                    && n.ssid == self.wifi_status.ssid
                {
                    self.confirm = Some(crate::confirm::Ask::WifiForget);
                    return vec![];
                }
                if matches!(n.security, crate::wifi::Security::Wep) {
                    self.notify(crate::i18n::text("WEP networks are not supported"));
                    return vec![];
                }
                if n.security.needs_password() {
                    self.wifi_join_target = Some(n.ssid);
                    self.open_keyboard(KbPurpose::WifiPassword);
                    return vec![];
                }
                self.wifi_join = Some((n.ssid, String::new()));
                self.wifi_status.state = State::Busy;
                crate::wifi::note_state(State::Busy);
                vec![Action::WifiJoin]
            }
            _ => vec![],
        }
    }

    /// The shell found (or did not find) the Wi-Fi install option. Without it the row is hidden.
    pub fn set_wifi_installed(&mut self, on: bool) {
        crate::wifi::set_installed(on);
        self.menu_idx = self.menu_idx.min(self.menu_len().saturating_sub(1));
    }

    /// `cinder-wifi status` output, after any Wi-Fi command or on the shell's own poll. `note`: the
    /// helper's last line when a command failed, shown in the strip.
    pub fn set_wifi_status(&mut self, text: &str, note: &str) {
        let mut st = crate::wifi::parse_status(text);
        // The helper's lines are lower-case sentences ("no saved network"); the strip starts with
        // a capital like every other note.
        let mut cs = note.trim().chars();
        st.note = cs
            .next()
            .map(|c| c.to_uppercase().chain(cs.take(59)).collect())
            .unwrap_or_default();
        if matches!(st.state, crate::wifi::State::Off) {
            self.wifi_networks.clear();
            self.wifi_scanning = false;
        }
        // The remote serves on the Wi-Fi link only; with the link gone it goes too.
        if self.remote_on
            && !matches!(
                st.state,
                crate::wifi::State::Connected | crate::wifi::State::Busy
            )
        {
            self.remote_on = false;
            self.remote_code.clear();
            self.remote_dropped = true;
        }
        crate::wifi::note_state(st.state);
        self.wifi_wps = false;
        self.wifi_status = st;
    }

    /// `cinder-wifi scan` output.
    pub fn set_wifi_scan(&mut self, text: &str) {
        self.wifi_networks = crate::wifi::parse_scan(text);
        self.wifi_scanning = false;
    }

    /// Settings ▸ Wi-Fi ▸ Options ▸ Share the drive (on until the player restarts).
    pub fn wifi_drive(&self) -> bool {
        self.opt(Opt::WifiDrive)
    }
    pub fn drive_note(&self) -> &str {
        &self.drive_note
    }
    pub fn set_drive_note(&mut self, note: &str) {
        self.drive_note = note.chars().take(60).collect();
    }
    /// The code in the shared drive's address ("" until it is first switched on).
    pub fn drive_code(&self) -> &str {
        &self.drive_code
    }
    pub fn set_drive_code(&mut self, code: &str) {
        self.drive_code = code
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .take(16)
            .collect();
    }
    pub fn play_to_note(&self) -> &str {
        &self.play_to_note
    }
    /// What the renderer is doing, for the switch's second line.
    pub fn set_play_to_note(&mut self, note: &str) {
        self.play_to_note = note.chars().take(60).collect();
    }
    /// Wi-Fi is connected (the last status said so).
    pub fn wifi_connected(&self) -> bool {
        matches!(self.wifi_status.state, crate::wifi::State::Connected)
    }

    /// What the AirPlay receiver is doing ("Shows up as Walkman (Cinder)", "Playing from AirPlay").
    pub fn set_airplay_note(&mut self, note: &str) {
        self.airplay_note = note.chars().take(60).collect();
    }

    /// What the last ListenBrainz send said ("Sent 12 songs", "ListenBrainz refused the token").
    pub fn set_wifi_lb_note(&mut self, note: &str) {
        self.wifi_lb_note = note.chars().take(60).collect();
    }
    /// What the last update check said ("Cinder v0.3.23 is out — update with the installer").
    pub fn set_wifi_update_note(&mut self, note: &str) {
        self.wifi_update_note = note.chars().take(60).collect();
    }
    /// The release the update check found newer ("v0.4.1"), or "" for none.
    pub fn set_update_offer(&mut self, tag: &str) {
        self.update_offer = tag.chars().take(24).collect();
    }
    pub fn update_offer(&self) -> &str {
        &self.update_offer
    }
    /// The install's progress ("Downloading Cinder v0.4.1…"); `stopped` = it failed and the row
    /// can be tapped again.
    pub fn set_update_install_note(&mut self, note: &str, stopped: bool) {
        self.update_install_note = note.chars().take(60).collect();
        if stopped {
            self.update_running = false;
        }
    }
    /// The install strip is on the Wi-Fi page: a newer release is known and the link is up.
    /// Under 30% with no cable in. A gauge that has not reported yet (no status) does not block,
    /// and "Not charging" is a cable in with the charge held (Battery care), which is fine.
    fn too_flat_to_update(&self) -> bool {
        let on_cable = matches!(
            self.batt_status.as_str(),
            "Charging" | "Full" | "Not charging"
        );
        !self.batt_status.is_empty() && self.batt_pct < 30 && !on_cable
    }

    fn update_offered(&self) -> bool {
        !self.update_offer.is_empty()
            && matches!(self.wifi_status.state, crate::wifi::State::Connected)
    }

    /// The join the last `WifiJoin` asked for: SSID bytes and password. Taken once.
    pub fn take_wifi_join(&mut self) -> Option<(Vec<u8>, String)> {
        self.wifi_join.take()
    }

    /// What "Save Up Next" saves: the playing track and everything after it, in play order, as
    /// object ids. What has already played is history, not part of what you asked to hear.
    pub fn queue_to_save(&self) -> Vec<i64> {
        self.context
            .get(self.context_idx..)
            .unwrap_or(&[])
            .iter()
            .map(|s| s.object_id)
            .collect()
    }

    /// The name the Save keyboard opens with.
    fn queue_save_name(&self) -> String {
        let rest = self.context.get(self.context_idx..).unwrap_or(&[]);
        match rest.first() {
            // `art` is the album name (the NEXT heading's test, the same one).
            Some(first) if !first.art.is_empty() && rest.iter().all(|s| s.art == first.art) => {
                first.art.clone()
            }
            _ => crate::i18n::text("Up Next").to_string(),
        }
    }

    /// "Add to playlist": row 0 makes a new one, the rest add to an existing one.
    fn tap_playlist_pick(&mut self, y: i32) -> Vec<Action> {
        let targets = self.user_playlists();
        match crate::playlist_pick::hit_row(targets.len() + 1, self.pick_scroll_px, y) {
            Some(0) => {
                self.open_keyboard(if self.pick_many.is_empty() {
                    KbPurpose::NewPlaylistWith(self.pick_track)
                } else {
                    KbPurpose::NewPlaylistWithPicked
                });
                vec![]
            }
            Some(index) if !self.pick_many.is_empty() => {
                let (id, name, _) = targets[index - 1].clone();
                let n = self.pick_many.len();
                self.pop();
                self.notify_format(
                    "Added {n} songs to {name}",
                    vec![("n", n.to_string()), ("name", name)],
                );
                vec![Action::PlaylistAddPicked(id)]
            }
            Some(index) => {
                let (id, name, _) = targets[index - 1].clone();
                let track = self.pick_track;
                self.pop();
                self.notify_format("Added to {name}", vec![("name", name)]);
                vec![Action::PlaylistAddTrack(id, track)]
            }
            None => vec![],
        }
    }

    /// "Add tracks": each tap adds one. The screen stays open — building a playlist is a run of
    /// taps, and popping back to the page after every one would make adding ten tracks ten trips.
    fn tap_track_pick(&mut self, y: i32) -> Vec<Action> {
        let Some(id) = self.playlist_row().map(|p| p.id) else {
            return vec![];
        };
        if crate::playlist_pick::hit_search(y) {
            self.open_keyboard(KbPurpose::TrackSearch);
            return vec![];
        }
        let count = self.track_pick_order.len();
        let Some(row) = crate::playlist_pick::hit_track_row(count, self.track_pick_scroll_px, y)
        else {
            return vec![];
        };
        let Some(song) = self
            .track_pick_songs()
            .get(row)
            .map(|s| (s.object_id, s.title.clone()))
        else {
            return vec![];
        };
        // Already in it? Say so rather than silently doing nothing — the tick is small.
        if self
            .playlist_row()
            .map(|p| p.track_list.iter().any(|s| s.object_id == song.0))
            .unwrap_or(false)
        {
            self.notify(crate::i18n::text("Already in this playlist"));
            return vec![];
        }
        self.notify_format("Added {}", vec![("0", song.1.clone())]);
        vec![Action::PlaylistAddTrack(id, song.0)]
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
        match library::albums_hit_at(
            &self.lib,
            self.album_sort,
            self.album_expanded,
            self.lib_scroll_px,
            x,
            y,
            self.lib_band(),
        ) {
            Some(AlbumsHit::AlbumToggle(flat)) => {
                // KEEP THE TAPPED ROW UNDER THE FINGER.
                //
                // Only one album is open at a time, so opening B also CLOSES A — and if A sat above
                // B, closing it deletes its whole track list from the content above B, dragging B
                // (and everything below) upward by that height. The album you just pressed then
                // jumps off the top of the view and you have to scroll back to find it. Reported
                // 2026-08-19: "the drop down doesn't open in the correct way and I have to scroll
                // to see the button I just pressed — it needs to push the rest of the library down."
                //
                // The row's content-space top is cheap to ask for either side of the change, so
                // shift the scroll by exactly the amount the row moved. The tapped row then stays
                // put and the rest of the library moves around it, which is what a disclosure
                // control is supposed to look like.
                // Only the case that actually moves the row costs anything: an album ALREADY OPEN
                // ABOVE this one, whose closing removes content from above it. With nothing open,
                // or the open one below, the row cannot move and the two layout builds here (which
                // are a full `albums_build` each, over every album) would be pure waste on the
                // input thread.
                let rank = self.album_rank_of(flat);
                let shifts = match (self.album_expanded, rank) {
                    (Some(open), Some(r)) if open != flat => {
                        self.album_rank_of(open).is_some_and(|o| o < r)
                    }
                    _ => false,
                };
                let before = rank.filter(|_| shifts).map(|r| {
                    library::row_top_px(
                        Tab::Albums,
                        &self.lib,
                        r,
                        self.album_sort,
                        self.album_expanded,
                    )
                });
                self.album_expanded = if self.album_expanded == Some(flat) {
                    None
                } else {
                    Some(flat)
                };
                if let Some(rank) = rank {
                    self.lib_idx = rank;
                }
                if let (Some(b), Some(r)) = (before, rank) {
                    let after = library::row_top_px(
                        Tab::Albums,
                        &self.lib,
                        r,
                        self.album_sort,
                        self.album_expanded,
                    );
                    self.lib_scroll_px += after - b;
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
                let id = self
                    .lib
                    .albums_flat()
                    .get(flat)
                    .and_then(|al| al.track_list.get(track))
                    .map(|s| s.object_id);
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
        match library::albums_hit_at(
            &self.lib,
            self.album_sort,
            self.album_expanded,
            self.lib_scroll_px,
            240,
            y,
            self.lib_band(),
        ) {
            Some(AlbumsHit::Track(flat, track)) => self
                .lib
                .albums_flat()
                .get(flat)
                .and_then(|al| al.track_list.get(track))
                .cloned(),
            _ => None,
        }
    }

    /// The album under `y` on the artist page, for the swipe-to-queue gesture.
    fn artist_album_at(&self, y: i32) -> Option<AlbumRow> {
        let page = self.artist_page()?;
        match library::artist_hit(&page, self.artist_scroll_px, y) {
            Some(library::ArtistHit::Album(flat)) => {
                self.lib.albums_flat().get(flat).map(|a| (*a).clone())
            }
            _ => None,
        }
    }

    /// The album under `y` in the Albums accordion header row, for the swipe-to-queue gesture.
    fn albums_album_at(&self, y: i32) -> Option<AlbumRow> {
        use crate::library::AlbumsHit;
        // A grid line holds four albums, and a swipe has no column: which one it queued would be
        // decided by where x=240 happens to fall. The grid's tiles do not swipe.
        if library::view_of(&self.lib, Tab::Albums) == library::LibView::Grid {
            return None;
        }
        match library::albums_hit_at(
            &self.lib,
            self.album_sort,
            self.album_expanded,
            self.lib_scroll_px,
            240,
            y,
            self.lib_band(),
        ) {
            Some(AlbumsHit::AlbumToggle(flat) | AlbumsHit::AlbumOpen(flat)) => {
                self.lib.albums_flat().get(flat).map(|a| (*a).clone())
            }
            _ => None,
        }
    }

    /// Which library tab is at screen-x `x`, from the strip as last rendered. Falls back to an
    /// even quarter split if nothing has been drawn yet (unreachable in practice — a tap cannot
    /// precede the first paint).
    fn tab_at_cached(&self, x: i32) -> Option<Option<Tab>> {
        let zones = self.lib_tab_zones.borrow();
        if zones.is_empty() {
            return Some(Some(match x * 4 / crate::canvas::W as i32 {
                0 => Tab::Songs,
                1 => Tab::Albums,
                2 => Tab::Artists,
                _ => Tab::Playlists,
            }));
        }
        tab_zone_at(&zones, x)
    }

    /// The header's view button: step the current tab to its next layout, keep the item that was at
    /// the top of the screen at the top, and say which view it is now.
    fn cycle_lib_view(&mut self) {
        let tab = self.lib_tab;
        let cur = library::view_of(&self.lib, tab);
        let next = library::next_view(tab, cur);
        // The first item on screen, by the layout being left — so the switch lands where you were,
        // not at the top of a 3,000-song list.
        let top_item = self.lib_top_item();
        self.lib_views[tab as usize] = next;
        self.lib.views = self.lib_views;
        self.album_expanded = None;
        self.lib_scroll_px = top_item
            .map(|i| library::row_top_px(tab, &self.lib, i, self.album_sort, None))
            .unwrap_or(0);
        self.clamp_lib_scroll();
        self.lib_band_hide = 0;
        self.fling_v = 0.0;
        self.notify(next.label());
    }

    /// Display rank of the first item whose line is at or below the top of the list.
    fn lib_top_item(&self) -> Option<usize> {
        let n = library::row_count(self.lib_tab, &self.lib);
        (0..n).find(|&i| {
            library::row_top_px(
                self.lib_tab,
                &self.lib,
                i,
                self.album_sort,
                self.album_expanded,
            ) >= self.lib_scroll_px
        })
    }

    /// Each tab's layout as the settings file stores it: four words, Songs to Playlists.
    pub fn lib_views_str(&self) -> String {
        self.lib_views
            .iter()
            .map(|v| v.token())
            .collect::<Vec<_>>()
            .join(",")
    }

    /// The Library's tab, Songs sort and Albums order as the settings file stores them
    /// (`lib_place=`), so the Library opens as it was left whatever Start on says.
    pub fn lib_place_str(&self) -> String {
        format!(
            "{}|{}|{}",
            tab_token(self.lib_tab),
            self.lib_sort,
            self.album_sort
        )
    }

    /// Read `lib_place_str`'s format. Out-of-range numbers are clamped; a short or garbled value
    /// changes nothing.
    pub fn set_lib_place_str(&mut self, s: &str) {
        let f: Vec<&str> = s.split('|').collect();
        let [tab, sort, album_sort] = f[..] else {
            return;
        };
        let (Ok(sort), Ok(album_sort)) = (
            sort.trim().parse::<usize>(),
            album_sort.trim().parse::<usize>(),
        ) else {
            return;
        };
        self.lib_tab = tab_from_token(tab);
        self.lib_sort = sort.min(library::SORTS.len() - 1);
        self.album_sort = album_sort.min(library::ALBUM_SORTS.len() - 1);
    }

    /// Read `lib_views_str`'s format. An unknown word leaves that tab as it was; a view the tab
    /// does not offer is kept but drawn as the list (`library::view_of`).
    pub fn set_lib_views_str(&mut self, s: &str) {
        for (i, w) in s.split(',').take(4).enumerate() {
            if let Some(v) = library::LibView::from_token(w) {
                self.lib_views[i] = v;
            }
        }
        self.lib.views = self.lib_views;
    }

    /// Advance the Albums ORDER chip and collapse any open accordion (its identity is still valid,
    /// but the position shift is less confusing when it re-opens deliberately).
    fn cycle_album_sort(&mut self) {
        self.album_sort = library::next_album_sort(&self.lib, self.album_sort);
        self.album_expanded = None;
    }

    /// The album DISPLAY rank (0-based over albums, under the current ORDER) of a `albums_flat()`
    /// index — for keeping the button cursor on a toggled row.
    fn album_rank_of(&self, flat: usize) -> Option<usize> {
        library::album_display_order(&self.lib, self.album_sort)
            .iter()
            .position(|&f| f == flat)
    }

    /// Largest useful library scroll for the current tab (Albums depends on ORDER + expansion).
    fn lib_max_scroll(&self) -> i32 {
        library::max_scroll_px(
            self.lib_tab,
            &self.lib,
            self.album_sort,
            self.album_expanded,
        )
    }

    /// Re-clamp the library scroll after the content height changes (accordion open/close).
    fn clamp_lib_scroll(&mut self) {
        let max = self.lib_max_scroll();
        self.lib_scroll_px = self.lib_scroll_px.clamp(0, max);
    }

    /// The shuffle band's slide as drawn and hit-tested RIGHT NOW. Every reader goes through this so
    /// the renderer and the tap handler can never disagree about where the band is.
    fn lib_band(&self) -> i32 {
        library::band_offset(self.lib_tab, self.lib_band_hide, self.lib_scroll_px)
    }

    /// Live drag-scroll of the current list by `dy_px` PIXELS (positive = content moves up /
    /// show later rows). Called per pump tick while a vertical drag is in progress, so the list
    /// tracks the finger; clamped to the content height.
    pub fn scroll_px(&mut self, dy_px: i32) {
        // The Shelf is a MODAL bottom sheet: it owns the gesture. Without this guard a drag that
        // started on the sheet scrolled (and flung) the list behind it, which read as the Shelf
        // "not working" — the sheet sat still while the screen moved underneath it.
        if self.overlay_open() || self.locked {
            return;
        }
        match self.current() {
            Screen::Library => {
                let max = self.lib_max_scroll();
                let before = self.lib_scroll_px;
                self.lib_scroll_px = (self.lib_scroll_px + dy_px).clamp(0, max);
                // The band follows what the LIST actually did, not what the finger asked for: a
                // drag past the end moves nothing and must slide nothing.
                let moved = self.lib_scroll_px - before;
                self.lib_band_hide = library::band_offset(
                    self.lib_tab,
                    self.lib_band_hide + moved,
                    self.lib_scroll_px,
                );
            }
            Screen::Album => {
                if let Some(al) = self.lib.albums_flat().get(self.album_view) {
                    let max = library::album_max_scroll_px(al);
                    self.album_scroll_px = (self.album_scroll_px + dy_px).clamp(0, max);
                }
            }
            Screen::Artist => {
                if let Some(max) = self
                    .artist_page()
                    .map(|p| library::artist_max_scroll_px(&p))
                {
                    self.artist_scroll_px = (self.artist_scroll_px + dy_px).clamp(0, max);
                }
            }
            Screen::Playlist => {
                if let Some(max) = self.playlist_row().map(library::playlist_max_scroll_px) {
                    self.playlist_scroll_px = (self.playlist_scroll_px + dy_px).clamp(0, max);
                }
            }
            Screen::Help => {
                let max = crate::help::max_scroll(self.opt(Opt::Quick));
                self.help_scroll_px = (self.help_scroll_px + dy_px).clamp(0, max);
            }
            Screen::Language => {
                let max = crate::language::max_scroll(self.languages.len());
                self.language_scroll_px = (self.language_scroll_px + dy_px).clamp(0, max);
            }
            Screen::Skins => {
                let max = crate::language::max_scroll(self.skin_names.len());
                self.skins_scroll_px = (self.skins_scroll_px + dy_px).clamp(0, max);
            }
            Screen::MediaServers => {
                if let Some(l) = self.media_levels.last_mut() {
                    let max = crate::media_server::max_scroll(l.entries.len());
                    l.scroll = (l.scroll + dy_px).clamp(0, max);
                }
            }
            Screen::Palette => {
                let max = crate::palette_list::max_scroll(
                    self.palettes.len() + 1,
                    self.palette_skipped.len(),
                );
                self.palette_scroll_px = (self.palette_scroll_px + dy_px).clamp(0, max);
            }
            Screen::Settings => {
                let max = crate::settings::max_scroll_px(self.settings_page, self.settings_lang());
                self.settings_scroll_px = (self.settings_scroll_px + dy_px).clamp(0, max);
            }
            Screen::Advanced => {
                let max = crate::advanced::max_scroll(self.clear_bass_available);
                self.adv_scroll_px = (self.adv_scroll_px + dy_px).clamp(0, max);
            }
            Screen::Presets => {
                let max = crate::presets::max_scroll(self.presets.len());
                self.presets_scroll_px = (self.presets_scroll_px + dy_px).clamp(0, max);
            }
            Screen::NetRadio => {
                let max = crate::netradio::max_scroll(self.radio_stations.len());
                self.radio_scroll_px = (self.radio_scroll_px + dy_px).clamp(0, max);
            }
            Screen::Sound => {
                let max = self.sound_max_scroll();
                self.sound_scroll_px = (self.sound_scroll_px + dy_px).clamp(0, max);
            }
            Screen::Device => {
                // The extent depends on the CONTENT (the care footnote is only drawn when care is
                // on), so it is computed from the same view the render uses rather than a constant.
                let max = crate::device::max_scroll_px(&self.device_view());
                self.device_scroll_px = (self.device_scroll_px + dy_px).clamp(0, max);
            }
            Screen::Storage => {
                let max = crate::storage::max_scroll_px(&self.storage_view());
                self.storage_scroll_px = (self.storage_scroll_px + dy_px).clamp(0, max);
            }
            Screen::Display => {
                let max = crate::display::max_scroll_px();
                self.display_scroll_px = (self.display_scroll() + dy_px).clamp(0, max);
            }
            Screen::Menu => {
                let max = crate::menu::max_scroll_px(self.menu_len());
                self.menu_scroll_px = (self.menu_scroll() + dy_px).clamp(0, max);
            }
            Screen::BtCodec => {
                let max = crate::bluetooth::codec_max_scroll(
                    crate::bluetooth::shows_ldac(self.bt_codec),
                    self.bt_aac,
                    self.bt_sbc_xq,
                );
                self.bt_codec_scroll_px = (self.bt_codec_scroll() + dy_px).clamp(0, max);
            }
            Screen::PlaylistPick => {
                let max = self.pick_max_scroll();
                self.pick_scroll_px = (self.pick_scroll_px + dy_px).clamp(0, max);
            }
            Screen::PlaylistCover => {
                let max = crate::playlist_cover::max_scroll(self.cover_choices().len());
                self.cover_scroll_px = (self.cover_scroll_px + dy_px).clamp(0, max);
            }
            Screen::TrackPick => {
                let max = self.track_pick_max_scroll();
                self.track_pick_scroll_px = (self.track_pick_scroll_px + dy_px).clamp(0, max);
            }
            Screen::PlaylistEdit => {
                if let Some(e) = self.pl_edit.as_mut() {
                    let max = crate::playlist_edit::max_scroll(e.order.len());
                    e.scroll_px = (e.scroll_px + dy_px).clamp(0, max);
                }
            }
            Screen::Search => {
                let max = self.search_max_scroll();
                self.search_scroll_px = (self.search_scroll_px + dy_px).clamp(0, max);
            }
            Screen::GenreFilter => {
                let max = library::genre_max_scroll_px(&self.lib);
                self.genre_scroll_px = (self.genre_scroll_px + dy_px).clamp(0, max);
            }
            Screen::TrackInfo => {
                let max = self.track_info_max_scroll();
                self.track_info_scroll_px = (self.track_info_scroll_px + dy_px).clamp(0, max);
            }
            Screen::Lyrics => {
                let max = self.lyrics_max_scroll();
                self.lyrics_scroll_px = (self.lyrics_scroll_px + dy_px).clamp(0, max);
                // Reading ahead, or back: the song must stop dragging the page out from under you.
                if dy_px != 0 {
                    self.lyrics_follow = false;
                }
            }
            Screen::Folders => {
                let max = crate::folders::max_scroll_px(&self.lib, self.folder_cur());
                self.folder_scroll_px = (self.folder_scroll_px + dy_px).clamp(0, max);
            }
            Screen::SensMe => {
                let max = crate::sensme::max_scroll_px(&self.lib, self.sensme_channel);
                self.sensme_scroll_px = (self.sensme_scroll_px + dy_px).clamp(0, max);
            }
            Screen::Books => {
                let max = crate::audiobooks::max_scroll_px(&self.lib);
                self.books_scroll_px = (self.books_scroll_px + dy_px).clamp(0, max);
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
        if self.overlay_open() || self.locked {
            return; // the modal sheet owns the gesture — see scroll_px
        }
        if matches!(
            self.current(),
            Screen::Library
                | Screen::Album
                | Screen::Artist
                | Screen::Playlist
                | Screen::UpNext
                | Screen::SensMe
                | Screen::Books
        ) {
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

    /// Now Playing's layout as it stands: this style, this mode, this song.
    /// The Menu's scroll, clamped like Display's.
    fn menu_scroll(&self) -> i32 {
        self.menu_scroll_px
            .min(crate::menu::max_scroll_px(self.menu_len()))
            .max(0)
    }

    /// Sound quality's scroll, clamped: Big view off, or a codec without the LDAC rows, leaves less
    /// (or nothing) to scroll.
    fn bt_codec_scroll(&self) -> i32 {
        self.bt_codec_scroll_px
            .min(crate::bluetooth::codec_max_scroll(
                crate::bluetooth::shows_ldac(self.bt_codec),
                self.bt_aac,
                self.bt_sbc_xq,
            ))
            .max(0)
    }

    /// Display's scroll, clamped: turning Big view off leaves nothing to scroll.
    fn display_scroll(&self) -> i32 {
        self.display_scroll_px
            .min(crate::display::max_scroll_px())
            .max(0)
    }

    fn np_layout(&self) -> crate::now_playing::Layout {
        crate::now_playing::layout(self.style, self.night, self.lyrics_button())
    }

    /// Rail x → permille (0..1000), along the rail the style draws, so the preview position and the
    /// drawn handle can never disagree.
    fn rail_permille(&self, x: i32, y: i32) -> u16 {
        if self.winamp_view() {
            return crate::winamp::seek_permille(crate::winamp::to_land(x, y).0);
        }
        (self.np_layout().rail.fraction(x) * 1000.0).round() as u16
    }

    /// Where the seek being dragged is, in permille; the shell turns it into milliseconds.
    pub fn scrub_permille(&self) -> u16 {
        self.scrub_permille
    }

    /// The shell has (or no longer has) a Winamp skin loaded.
    pub fn set_winamp_skin(&mut self, loaded: bool) {
        self.winamp_skin = loaded;
    }

    /// Now Playing is the Winamp skin's landscape view (`winamp.rs`).
    pub fn winamp_view(&self) -> bool {
        self.winamp_skin && self.style == crate::style::Style::Winamp
    }

    /// The shell has (or no longer has) a cassette tape loaded.
    pub fn set_tape_loaded(&mut self, loaded: bool) {
        self.tape_loaded = loaded;
    }

    /// Now Playing is a cassette tape on its side (`tape.rs`).
    pub fn tape_view(&self) -> bool {
        self.tape_loaded && self.style == crate::style::Style::Cassette
    }

    /// The skin's display choices, for the settings file (`wa=`): remaining time, the visualiser
    /// (spectrum, oscilloscope, off) and the analyzer's look.
    pub fn winamp_prefs(&self) -> String {
        format!(
            "{},{},{}",
            self.wa_remaining as u8, self.wa_vis, self.wa_style
        )
    }
    pub fn set_winamp_prefs(&mut self, v: &str) {
        let n: Vec<u8> = v.split(',').filter_map(|p| p.trim().parse().ok()).collect();
        if let [rem, vis, style] = n[..] {
            self.wa_remaining = rem != 0;
            self.wa_vis = vis % 3;
            self.wa_style = style % crate::winamp::ANALYZER_STYLES.len() as u8;
        }
    }

    /// The Winamp playlist's first row: where it was scrolled to, else the playing track.
    fn wa_pl_first(&self) -> usize {
        self.wa_pl_first.unwrap_or(self.context_idx)
    }

    /// The skin's volume slider moved to `lx`. On the jack the level is set outright, and the
    /// shell writes whatever level it reads back; on Bluetooth, where a step is relative, one step
    /// toward the finger per move.
    fn wa_volume_to(&mut self, lx: i32) -> Vec<Action> {
        if self.bt_route {
            let want = crate::winamp::volume_at(lx, crate::overlay::BT_VOL_MAX);
            if want == self.bt_volume {
                return vec![];
            }
            let up = want > self.bt_volume;
            self.vol_step(up);
            return vec![if up { Action::VolUp } else { Action::VolDown }];
        }
        let want = crate::winamp::volume_at(lx, crate::overlay::VOL_MAX);
        if want == self.volume || self.line_out_holds() {
            return vec![];
        }
        self.volume = want;
        vec![Action::VolUp]
    }

    /// A tap on the Winamp skin's landscape view. The table in `winamp.rs` says what each does;
    /// the sliders are not here, `scrub_begin` takes them.
    fn winamp_tap(&mut self, x: i32, y: i32) -> Vec<Action> {
        use crate::winamp::{self, Ctl};
        let (lx, ly) = winamp::to_land(x, y);
        let open = |a: &mut Self, s: Screen| {
            a.push(s);
            vec![]
        };
        match winamp::ctl_at(lx, ly, self.wa_playlist) {
            Some(Ctl::Menu) => open(self, Screen::Menu),
            Some(Ctl::Minimize | Ctl::Close) => open(self, Screen::Library),
            // ⏏ is Winamp's "open files": the library.
            Some(Ctl::Eject) => open(self, Screen::Library),
            // The playlist takes the whole panel, and gives it back (Winamp's PL button).
            Some(Ctl::Shade | Ctl::Pl) => {
                self.wa_playlist = !self.wa_playlist;
                self.wa_pl_first = None;
                vec![]
            }
            Some(Ctl::Options) => open(self, Screen::Settings),
            Some(Ctl::OnTop) => open(self, Screen::Display),
            Some(Ctl::FileInfo | Ctl::Title) => {
                self.track_info_scroll_px = 0;
                open(self, Screen::TrackInfo)
            }
            Some(Ctl::Double) => vec![Action::NextSkin],
            // Winamp's V opens its visualisation menu; here it steps the analyzer's look.
            Some(Ctl::VisMenu) => {
                let n = crate::winamp::ANALYZER_STYLES.len() as u8;
                self.wa_style = (self.wa_style + 1) % n;
                self.wa_vis = 0;
                let name = crate::winamp::ANALYZER_STYLES[self.wa_style as usize];
                self.notify_raw(&format!("Analyzer: {name}"));
                vec![]
            }
            Some(Ctl::Vis) => {
                self.wa_vis = (self.wa_vis + 1) % 3;
                vec![]
            }
            Some(Ctl::Time) => {
                self.wa_remaining = !self.wa_remaining;
                vec![]
            }
            Some(Ctl::Eq) => open(self, Screen::Eq),
            Some(Ctl::Prev) => vec![Action::Prev],
            Some(Ctl::Next) => vec![Action::Next],
            Some(Ctl::Play) if !self.playing => {
                self.set_playing_optimistic(true);
                vec![Action::PlayPause]
            }
            Some(Ctl::Pause) if self.playing => {
                self.set_playing_optimistic(false);
                vec![Action::PlayPause]
            }
            Some(Ctl::Stop) => {
                self.set_playing_optimistic(false);
                vec![Action::Stop]
            }
            Some(Ctl::Shuffle) => vec![Action::ShuffleToggle],
            Some(Ctl::Repeat) => vec![Action::RepeatCycle],
            // A row jumps to its track, as Up Next's rows do; the playing one does nothing.
            Some(Ctl::PlRow(i)) => {
                let qi = self.wa_pl_first() + i;
                if qi == self.context_idx || qi >= self.context.len() {
                    return vec![];
                }
                self.queue_follow = true;
                self.wa_pl_first = None;
                vec![Action::PlayContextAt(qi)]
            }
            _ => vec![],
        }
    }

    /// A finger went down at (x, y). True if it grabbed a scrubbable control.
    pub fn scrub_begin(&mut self, x: i32, y: i32) -> bool {
        self.scrub = Scrub::None;
        if self.locked || self.overlay_open() {
            return false;
        }
        match self.current() {
            // A tape has nothing to drag.
            Screen::NowPlaying if self.tape_view() => false,
            Screen::NowPlaying if self.winamp_view() => {
                use crate::winamp::{self, Ctl};
                let (lx, ly) = winamp::to_land(x, y);
                self.scrub = match winamp::ctl_at(lx, ly, self.wa_playlist) {
                    Some(Ctl::Seek) => Scrub::Progress,
                    Some(Ctl::Volume) => Scrub::WaVolume,
                    Some(Ctl::Balance) => Scrub::WaBalance,
                    Some(Ctl::Title) => Scrub::WaTitle {
                        x0: x,
                        y0: y,
                        lx0: lx,
                        shift0: self.wa_title_shift,
                        moved: false,
                    },
                    _ => return false,
                };
                self.scrub_permille = self.rail_permille(x, y);
                true
            }
            Screen::NowPlaying if self.np_layout().rail.grabs(x, y) => {
                self.scrub = Scrub::Progress;
                self.scrub_permille = self.rail_permille(x, y);
                true
            }
            Screen::Display => {
                let row = (y >= crate::chrome::HEADER_BOTTOM)
                    .then(|| crate::display::row_at(y + self.display_scroll()))
                    .flatten();
                if row == Some(crate::display::ROW_SIZE) {
                    self.scrub = Scrub::UiScale;
                    self.display_sel = crate::display::ROW_SIZE;
                    crate::text::set_scale_idx(crate::display::size_idx_at(x));
                    true
                } else {
                    false
                }
            }
            Screen::Eq => {
                if (crate::eq::FIELD_TOP..crate::eq::FIELD_BOTTOM).contains(&y) {
                    if let Some(band) = crate::eq::band_at(x) {
                        self.scrub = Scrub::EqBand(band);
                        self.eq_sel = band;
                        self.eq_bands[band] = crate::eq::value_at_y(y);
                        return true;
                    }
                }
                false
            }
            Screen::Tone => {
                if (crate::tone::FIELD_TOP..crate::tone::FIELD_BOTTOM).contains(&y) {
                    if let Some(band) = crate::tone::band_at(x) {
                        self.scrub = Scrub::ToneBand(band);
                        self.tone_sel = band;
                        self.tone_bands[band] = crate::tone::value_at_y(y);
                        return true;
                    }
                }
                false
            }
            // The knob follows the finger; nothing is applied until release (scrub_end).
            Screen::DacEq => {
                if (crate::dac_eq::FIELD_TOP..crate::dac_eq::FIELD_BOTTOM).contains(&y) {
                    if let Some(band) = crate::dac_eq::band_at(x) {
                        self.scrub = Scrub::DacEqBand(band);
                        self.dac_eq_sel = band;
                        self.dac_eq[band] = crate::dac_eq::value_at_y(y);
                        return true;
                    }
                }
                false
            }
            Screen::VolCurve => {
                use crate::vol_curve as vc;
                if self.vol_curve_ok && (vc::FIELD_TOP..vc::FIELD_BOTTOM).contains(&y) {
                    if let Some(i) = vc::point_at(x) {
                        self.scrub = Scrub::VolCurvePoint(i);
                        self.vol_curve_sel = i;
                        vc::set(&mut self.vol_curve, i, vc::value_at_y(y));
                        return true;
                    }
                }
                false
            }
            Screen::Soundscape => match crate::soundscape::level_row_at(y) {
                Some(row) if crate::soundscape::slider_grab(x) => {
                    self.scrub = Scrub::AmbientLevel(row);
                    self.ambient_sel = crate::soundscape::SEL_ALONE + row;
                    self.set_ambient_level(row, crate::soundscape::level_at(x));
                    true
                }
                _ => false,
            },
            Screen::Sound
                if crate::sound::row_at(y, self.sound_scroll())
                    == Some(crate::sound::ROW_BALANCE)
                    && crate::sound::balance_grab(y, self.sound_scroll()) =>
            {
                self.scrub = Scrub::Balance;
                self.sound_sel = crate::sound::ROW_BALANCE;
                self.snd_balance = crate::sound::balance_at(x);
                true
            }
            _ => false,
        }
    }

    /// The finger moved to `x` during a scrub. The rail only PREVIEWS here (committing a seek per
    /// motion event would hammer PlayerService); the UI-scale slider applies live.
    pub fn scrub_move(&mut self, x: i32, y: i32) -> Vec<Action> {
        match self.scrub {
            // Live, and deliberately emitting NOTHING when the value did not change: a drag
            // produces a motion event per frame, but only ~20 of them land on a new step. That
            // check is most of the fix for "adjusting the EQ stutters the audio" — the rest is in
            // the shell, which now writes only the band that actually moved.
            Scrub::EqBand(b) => {
                let want = crate::eq::value_at_y(y);
                if want == self.eq_bands[b] {
                    return vec![];
                }
                self.eq_bands[b] = want;
                // Off any named preset the moment a band is hand-edited — the pill would otherwise
                // keep claiming "ROCK" for a curve that is no longer ROCK.
                self.eq_preset = 0;
                vec![Action::EqChanged(self.eq_bands)]
            }
            Scrub::ToneBand(b) => {
                let want = crate::tone::value_at_y(y);
                if want == self.tone_bands[b] {
                    return vec![];
                }
                self.tone_bands[b] = want;
                vec![Action::SoundChanged]
            }
            Scrub::DacEqBand(b) => {
                self.dac_eq[b] = crate::dac_eq::value_at_y(y);
                vec![]
            }
            Scrub::VolCurvePoint(i) => {
                crate::vol_curve::set(&mut self.vol_curve, i, crate::vol_curve::value_at_y(y));
                vec![]
            }
            Scrub::Progress => {
                self.scrub_permille = self.rail_permille(x, y);
                vec![]
            }
            Scrub::WaVolume => self.wa_volume_to(crate::winamp::to_land(x, y).0),
            Scrub::WaTitle {
                x0,
                y0,
                lx0,
                shift0,
                moved,
            } => {
                // One character per cell of the skin's font; dragging left reads on.
                let lx = crate::winamp::to_land(x, y).0;
                let moved = moved || (lx - lx0).abs() >= crate::winamp::CELL;
                self.wa_title_shift = shift0 + (lx0 - lx) / crate::winamp::CELL;
                self.scrub = Scrub::WaTitle {
                    x0,
                    y0,
                    lx0,
                    shift0,
                    moved,
                };
                vec![]
            }
            Scrub::WaBalance => {
                let want = crate::winamp::balance_at(crate::winamp::to_land(x, y).0);
                if want == self.snd_balance {
                    return vec![];
                }
                self.snd_balance = want;
                vec![Action::BalanceChanged]
            }
            Scrub::UiScale => {
                crate::text::set_scale_idx(crate::display::size_idx_at(x));
                vec![]
            }
            // Live, unlike the rail: you have to hear a balance change to aim it. Emits nothing
            // when the finger moved inside the same slider step, so a slow drag doesn't queue an
            // action per motion event.
            Scrub::Balance => {
                let want = crate::sound::balance_at(x);
                if want == self.snd_balance {
                    return vec![];
                }
                self.snd_balance = want;
                vec![Action::BalanceChanged]
            }
            Scrub::AmbientLevel(row) => {
                if self.set_ambient_level(row, crate::soundscape::level_at(x)) {
                    vec![Action::AmbientChanged]
                } else {
                    vec![]
                }
            }
            Scrub::None => vec![],
        }
    }

    /// The finger lifted: commit the scrub.
    pub fn scrub_end(&mut self) -> Vec<Action> {
        let acts = match self.scrub {
            Scrub::Progress => vec![Action::Seek(self.scrub_permille)],
            Scrub::UiScale => vec![Action::UiScaleChanged],
            // Already applied on every move; the release only has to get the final value written to
            // the settings file, which SoundChanged does on top of re-applying the same balance.
            Scrub::Balance | Scrub::WaBalance => vec![Action::SoundChanged],
            // Applied on every move; on the jack the release writes the final level once more,
            // since the shell keeps only the newest of a drag's moves.
            Scrub::WaVolume if !self.bt_route => vec![Action::VolUp],
            Scrub::WaVolume => vec![],
            // Already applied on every step. The release only has to get the final curve into the
            // settings file, which is what these actions do on top of re-applying it.
            Scrub::EqBand(_) => vec![Action::EqChanged(self.eq_bands)],
            Scrub::ToneBand(_) => vec![Action::SoundChanged],
            // The only point this one is applied: one helper run per drag, not one per step.
            Scrub::DacEqBand(_) => vec![Action::DacEqChanged],
            // Dragging a point is choosing a curve: it switches the curve on.
            Scrub::VolCurvePoint(_) => {
                self.vol_curve_on = true;
                vec![Action::VolCurveChanged]
            }
            Scrub::AmbientLevel(_) => vec![Action::AmbientChanged],
            // A press on the title that never moved is the tap it always was: Track information.
            Scrub::WaTitle {
                x0,
                y0,
                moved: false,
                ..
            } => {
                self.scrub = Scrub::None;
                return self.winamp_tap(x0, y0);
            }
            Scrub::WaTitle { .. } => vec![],
            Scrub::None => vec![],
        };
        self.scrub = Scrub::None;
        acts
    }

    /// Is the live scrub the NOW-PLAYING PROGRESS RAIL — the only one whose gesture means a seek?
    ///
    /// The FFI used to ask the inverse question ("is it the UI-scale slider?") and treat everything
    /// else as the rail. That was fine while the rail and one slider were the only two, and it broke
    /// the moment a third arrived: the balance drag ran the rail's code path, so it never moved the
    /// balance, and on release it SEEKED THE TRACK. Asking which control is the special one, rather
    /// than listing the controls that are not, means a fourth slider is inert rather than dangerous.
    pub fn scrub_is_rail(&self) -> bool {
        matches!(self.scrub, Scrub::Progress)
    }

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

    /// Big view: rows and touch zones at `scale::LARGE_PCT`, and the text at least `BIG_TEXT_PCT`
    /// (owner, 10-25: "around 130-140 looks good for text"). Off puts the text back to 100 % only
    /// if it is still where Big view put it, so a size chosen on the slider is kept.
    pub fn set_big_view(&mut self, on: bool) {
        crate::scale::set_large(on);
        let pct = crate::text::scale_pct();
        if on && pct < BIG_TEXT_PCT {
            crate::text::set_scale_pct(BIG_TEXT_PCT);
        } else if !on && pct == BIG_TEXT_PCT {
            crate::text::set_scale_pct(100);
        }
    }
    pub fn big_view(&self) -> bool {
        crate::scale::large()
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
        if self.locked || self.overlay_open() || self.confirm.is_some() {
            return false;
        }
        if !self.row_swipes(y) {
            return false;
        }
        self.swipe_live = true;
        self.swipe_row = Some(crate::library::SwipeRow {
            y,
            dx: library::swipe_offset(dx),
        });
        true
    }

    /// A LONG PRESS that lifted nothing (the shell offers `reorder_begin_hold` first): on a song
    /// row it is Play next, the same as a leftward swipe, for anyone who finds a hold easier than
    /// a swipe. None where nothing would be queued, so the shell can let the contact go on as it
    /// was; Up Next is left out because a hold there moves the row and a swipe removes it.
    pub fn hold(&mut self, x: i32, y: i32) -> Option<Vec<Action>> {
        if self.locked || self.overlay_open() || self.confirm.is_some() {
            return None;
        }
        // A FOLDER row: start (or add to) a selection for "Add to playlist" (#56). Song rows
        // still Play next.
        if self.current() == Screen::Folders {
            if let Some(crate::folders::Row::Dir(i)) =
                crate::folders::row_at(&self.lib, self.folder_cur(), y, self.folder_scroll_px)
            {
                if !self.folder_sel.contains(&i) {
                    self.folder_sel.push(i);
                }
                return Some(vec![]);
            }
        }
        if !self.hold_play_next || self.current() == Screen::UpNext || !self.row_swipes(y) {
            return None;
        }
        Some(self.swipe(-1, x, y))
    }

    /// Is there a row under `y` that a horizontal swipe acts on?
    fn row_swipes(&self, y: i32) -> bool {
        // Only where a track sits under the finger: the same rows `swipe` can queue. Artist and
        // playlist rows have nothing to queue, so they must not move — a row that slides and then
        // does nothing is worse feedback than a row that never moved.
        match self.current() {
            Screen::Library => match self.lib_tab {
                Tab::Songs => library::hit_row_at(
                    self.lib_tab,
                    &self.lib,
                    self.lib_scroll_px,
                    y,
                    self.lib_band(),
                )
                .and_then(|r| library::song_at(&self.lib, self.lib_sort, r))
                .is_some(),
                Tab::Albums => {
                    self.albums_track_at(y).is_some() || self.albums_album_at(y).is_some()
                }
                _ => false,
            },
            Screen::Album => self
                .lib
                .albums_flat()
                .get(self.album_view)
                .and_then(|al| library::album_hit_track(al, self.album_scroll_px, y))
                .is_some(),
            Screen::Artist => {
                self.artist_track_at(y).is_some() || self.artist_album_at(y).is_some()
            }
            Screen::Playlist => self.playlist_track_at(y).is_some(),
            Screen::Search | Screen::Folders | Screen::SensMe => self.list_song_at(y).is_some(),
            // Up Next's own rows swipe too, but to REMOVE rather than to queue again — every row
            // still to come. The played ones are a record and the playing one is playing.
            Screen::UpNext => matches!(
                self.up_next_layout().at(y, self.queue_scroll_px),
                Some(crate::up_next::Slot::Upcoming(_))
            ),
            _ => false,
        }
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
        self.reorder_begin_inner(x, y, true)
    }

    /// The same pick-up, from ANYWHERE on a queue row, for a contact the shell has already
    /// classified as a long press.
    ///
    /// The grab handle exists because a vertical drag on this screen is ambiguous — reorder the row
    /// under the thumb, or scroll the list? Start-point ownership answers that instantly, at the
    /// cost of making reordering reachable only from one narrow column. A HOLD answers it too, and
    /// from anywhere: a finger that has stayed still for the hold interval is not scrolling. So the
    /// two live side by side — the handle lifts a row immediately, and a hold lifts one from any
    /// point on it — and neither can steal a contact from the other, because by the time this is
    /// offered the shell has already established that the finger did not move.
    pub fn reorder_begin_hold(&mut self, x: i32, y: i32) -> bool {
        self.reorder_begin_inner(x, y, false)
    }

    fn reorder_begin_inner(&mut self, x: i32, y: i32, require_grip: bool) -> bool {
        if self.locked || self.overlay_open() || self.confirm.is_some() {
            return false;
        }
        // The playlist editor's rows lift the same way: by the handle at once, or from anywhere
        // on a hold. Its list is flat, so the slot is plain arithmetic (`playlist_edit`).
        if self.current() == Screen::PlaylistEdit {
            let Some(e) = self.pl_edit.as_ref() else {
                return false;
            };
            if require_grip && !crate::playlist_edit::hit_grip(x) {
                return false;
            }
            let Some(from) = crate::playlist_edit::row_at(e.order.len(), e.scroll_px, y) else {
                return false;
            };
            self.fling_v = 0.0;
            self.row_drag = Some(crate::up_next::RowDrag {
                from,
                to: from,
                start_y: y,
                y,
                grab_off: y - crate::playlist_edit::row_top(from, e.scroll_px),
            });
            return true;
        }
        // Up Next is the only other screen with reorderable rows on it. What is reorderable THERE
        // is decided by the slot under the finger, below.
        if self.current() != Screen::UpNext {
            return false;
        }
        if require_grip && !crate::up_next::queue_grip_hit(x) {
            return false;
        }
        let _ = x;
        let lay = self.up_next_layout();
        // ONE index space over both sections — see `up_next`'s MOVABLE SPAN note. Every row
        // except the playing one can be picked up, and can land anywhere from straight after the
        // playing track to the end of the sequence.
        //
        // EXCEPT the playing row, which is not a position in a list, it is the present. And a row
        // can never land IN the history (`Layout::drop_first`), only come out of it. Because
        // nothing at or before `context_idx` can move, the index cannot be disturbed by a drag,
        // which is what makes this safe to do to a live sequence.
        //
        // The HANDLE is offered here iff the renderer drew one (`up_next::track_row` with
        // `grippy`). A strip that lifts a row where nothing indicates it is a trap, so the two
        // must stay in step.
        let slot = lay.at(y, self.queue_scroll_px);
        let Some(from) = slot.and_then(|s| lay.movable_index(s)) else {
            return false;
        };
        // The row's screen top comes from the LAYOUT, not from `from * RH` — the span is contiguous
        // in index but not in pixels (a section heading sits inside it), so that arithmetic would
        // grab the wrong offset and the lifted row would jump under the finger.
        let Some(content_top) = slot.and_then(|s| lay.top_of(s)) else {
            return false;
        };
        let row_top = crate::chrome::HEADER_BOTTOM + content_top - self.queue_scroll_px;
        self.fling_v = 0.0; // a pick-up must not ride a leftover flick
        self.row_drag = Some(crate::up_next::RowDrag {
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
        let Some(mut d) = self.row_drag else { return };
        d.y = d.start_y + dy;
        if let (Screen::PlaylistEdit, Some(e)) = (self.current(), self.pl_edit.as_ref()) {
            d.to = crate::playlist_edit::slot_for(e.order.len(), d.float_top(), e.scroll_px);
            self.row_drag = Some(d);
            return;
        }
        let lay = self.up_next_layout();
        // One span, so one question: which movable position is the floating row over?
        d.to = lay.movable_slot_for(d.from, d.float_top(), self.queue_scroll_px);
        self.row_drag = Some(d);
    }

    /// Drop the row. Returns `QueueChanged` when the order actually moved, since that changes what
    /// PlayerService should be playing next.
    pub fn reorder_release(&mut self) -> Vec<Action> {
        let Some(d) = self.row_drag.take() else {
            return vec![];
        };
        if self.current() == Screen::PlaylistEdit {
            // Nothing for the shell yet: the order is the editor's until DONE.
            if let Some(e) = self.pl_edit.as_mut() {
                e.move_row(d.from, d.to);
            }
            return vec![];
        }
        self.movable_move(d.from, d.to)
    }

    /// Move one row of the MOVABLE SPAN to any other position in it — the storage half of the drag
    /// (`up_next`'s MOVABLE SPAN note has the why).
    ///
    /// The span is `history ++ context[context_idx + 1 ..]`:
    ///
    /// | from | to | what it means |
    /// |---|---|---|
    /// | upcoming | upcoming | reorder what is coming |
    /// | history | upcoming | play something again — the reason history is draggable at all |
    /// | *anything* | history | **impossible, by construction** — see below |
    ///
    /// **HISTORY IS A SOURCE, NEVER A DESTINATION.** `up_next::Layout::drop_first` is the floor on
    /// every `to`. The list is a record of what was played; dropping a track that has not played
    /// into it would be writing something into that record which is not true. A history row that
    /// IS dragged out leaves the history — it is coming up now, and it will be recorded again when
    /// it plays again.
    ///
    /// **Remove-then-insert, exactly as `up_next::drag_order` previewed it.** `to` is an index in
    /// the span *after* `from` has been taken out, which is what makes the row land where the gap
    /// opened rather than one place past it.
    ///
    /// Nothing it touches is at or before `context_idx`, so the playing track keeps its position
    /// and what is playing is never disturbed. `pre_shuffle` is left alone for a reorder: it
    /// records the order to go back to when shuffle is switched OFF, and a hand reorder made while
    /// shuffled is an edit to the shuffled order, not to the original one. A row pulled out of the
    /// history is NEW to the sequence, though, so it is recorded there as coming next.
    fn movable_move(&mut self, from: usize, to: usize) -> Vec<Action> {
        let first = self.context_idx + 1;
        let hlen = self.history.len();
        let ulen = self.context.len().saturating_sub(first);
        let len = hlen + ulen;
        if from >= len || to >= len || self.context.is_empty() {
            return vec![];
        }
        // The history length AFTER the lift, and the floor it sets: never into the history,
        // whatever was asked for. Applied BEFORE the no-op test, or a clamped move would read as
        // "nothing moved" and be dropped.
        let h = hlen - usize::from(from < hlen);
        let to = to.max(h);
        if from == to {
            return vec![]; // remove at i, insert at i — the list is unchanged
        }
        let row = if from < hlen {
            let row = self.history.remove(from);
            self.pre_shuffle_insert_next(&[row.object_id]);
            row
        } else {
            self.context.remove(first + from - hlen)
        };
        // Clamped because a caller is not required to have read the table above.
        let at = (first + to - h).min(self.context.len());
        self.context.insert(at, row);
        vec![Action::QueueChanged]
    }

    /// The drag in effect, for tests and the host preview.
    pub fn reorder_state(&self) -> Option<crate::up_next::RowDrag> {
        self.row_drag
    }

    /// The Up Next slot list. Rebuilt on demand from the three numbers `render` publishes, rather
    /// than cached from the last frame: a hit test that depended on a frame having been drawn is a
    /// hit test that silently resolves against the wrong list the first time it runs.
    fn up_next_layout(&self) -> crate::up_next::Layout {
        crate::up_next::layout(
            self.history.len(),
            self.context.len(),
            (!self.context.is_empty()).then_some(self.context_idx),
        )
    }

    /// `(max_scroll_px, list_top)` of whatever the current screen scrolls, or None if it doesn't.
    /// The scrollbar's whole geometry follows from these two, so this is the only place a screen
    /// has to be taught about the drag.
    /// `(max_scroll, list_top, list_bottom)` of whatever the current screen scrolls, or None if it
    /// doesn't. The BOTTOM is part of it because not every scrolling screen ends where the
    /// library's list ends: the two pickers stop 24 px higher to make room for their footer note,
    /// and this used to assume `library::list_bottom()` for all of them — so their thumb travel was
    /// scaled against the wrong window and a drag to the end of the strip stopped short of the end
    /// of the list. Found by the 2026-09-06 UI audit.
    fn sbar_metrics(&self) -> Option<(i32, i32, i32)> {
        let lb = library::list_bottom();
        match self.current() {
            Screen::Library => Some((self.lib_max_scroll(), library::list_top(self.lib_tab), lb)),
            Screen::Album => self.lib.albums_flat().get(self.album_view).map(|al| {
                (
                    library::album_max_scroll_px(al),
                    library::album_tracks_top(),
                    lb,
                )
            }),
            Screen::Artist => self.artist_page().map(|p| {
                (
                    library::artist_max_scroll_px(&p),
                    library::artist_content_top(),
                    lb,
                )
            }),
            Screen::Playlist => self.playlist_row().map(|p| {
                (
                    library::playlist_max_scroll_px(p),
                    library::playlist_content_top(p),
                    lb,
                )
            }),
            // The bar rides the WHOLE list now (history + current + queue + album), not just the
            // user queue — which is also why it appears on a plain album view, where the old
            // row-stepping window offered no way to drag at all.
            Screen::UpNext => Some((
                self.up_next_layout().max_scroll_px(),
                crate::chrome::HEADER_BOTTOM,
                lb,
            )),
            Screen::GenreFilter => Some((
                library::genre_max_scroll_px(&self.lib),
                library::GENRE_TOP,
                lb,
            )),
            Screen::TrackInfo => Some((
                self.track_info_max_scroll(),
                crate::track_info::TOP,
                crate::track_info::BOTTOM,
            )),
            Screen::Lyrics => Some((
                self.lyrics_max_scroll(),
                crate::lyrics::TOP,
                crate::lyrics::BOTTOM,
            )),
            Screen::PlaylistPick => Some((
                self.pick_max_scroll(),
                crate::playlist_pick::TOP,
                crate::playlist_pick::BOTTOM,
            )),
            Screen::PlaylistCover => Some((
                crate::playlist_cover::max_scroll(self.cover_choices().len()),
                crate::playlist_cover::TOP,
                crate::playlist_cover::BOTTOM,
            )),
            Screen::Search => Some((
                self.search_max_scroll(),
                crate::playlist_pick::TOP,
                crate::playlist_pick::BOTTOM,
            )),
            Screen::TrackPick => Some((
                self.track_pick_max_scroll(),
                crate::playlist_pick::TOP,
                crate::playlist_pick::BOTTOM,
            )),
            Screen::PlaylistEdit => self.pl_edit.as_ref().map(|e| {
                (
                    crate::playlist_edit::max_scroll(e.order.len()),
                    crate::playlist_edit::LIST_TOP,
                    crate::playlist_edit::LIST_BOTTOM,
                )
            }),
            Screen::Folders => Some((
                crate::folders::max_scroll_px(&self.lib, self.folder_cur()),
                crate::folders::TOP,
                lb,
            )),
            // The channel page's list starts under the band, so the strip is measured against
            // `list_top`, not the header. Scaling a drag against the wrong window is exactly the
            // defect the 2026-09-06 audit found on both playlist pickers.
            Screen::SensMe => Some((
                crate::sensme::max_scroll_px(&self.lib, self.sensme_channel),
                crate::sensme::list_top(self.sensme_channel),
                if self.sensme_channel.is_none() {
                    crate::sensme::GRID_BOTTOM()
                } else {
                    lb
                },
            )),
            Screen::Books => Some((
                crate::audiobooks::max_scroll_px(&self.lib),
                crate::audiobooks::TOP,
                lb,
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
        if self.locked || self.overlay_open() || self.confirm.is_some() {
            return false;
        }
        if !library::sbar_hit_x(x) {
            return false;
        }
        let Some((max, top, bottom)) = self.sbar_metrics() else {
            return false;
        };
        // Nothing to scroll, or the thumb fills the track: no drag, so the contact stays available
        // to the list underneath.
        if max <= 0 || !(top..bottom).contains(&y) {
            return false;
        }
        if library::sbar_span(top, bottom, max + (bottom - top)) <= 0 {
            return false;
        }
        self.fling_v = 0.0;
        self.sbar = Some((self.sbar_scroll(), y));
        self.sbar_az = (self.current() == Screen::Library
            && library::az_key_for(self.lib_tab, self.lib_sort, self.album_sort).is_some())
        .then(|| {
            library::az_offsets(
                self.lib_tab,
                &self.lib,
                self.lib_sort,
                self.album_sort,
                self.album_expanded,
            )
        });
        true
    }

    /// Current scroll of the screen the scrollbar is riding.
    fn sbar_scroll(&self) -> i32 {
        match self.current() {
            Screen::Album => self.album_scroll_px,
            Screen::Artist => self.artist_scroll_px,
            Screen::Playlist => self.playlist_scroll_px,
            Screen::UpNext => self.queue_scroll_px,
            Screen::GenreFilter => self.genre_scroll_px,
            Screen::TrackInfo => self.track_info_scroll_px,
            Screen::Lyrics => self.lyrics_scroll_px,
            Screen::Folders => self.folder_scroll_px,
            Screen::SensMe => self.sensme_scroll_px,
            Screen::Books => self.books_scroll_px,
            Screen::PlaylistPick => self.pick_scroll_px,
            Screen::PlaylistCover => self.cover_scroll_px,
            Screen::TrackPick => self.track_pick_scroll_px,
            Screen::Search => self.search_scroll_px,
            Screen::PlaylistEdit => self.pl_edit.as_ref().map_or(0, |e| e.scroll_px),
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
        let Some((start_scroll, _)) = self.sbar else {
            return;
        };
        let Some((max, top, bottom)) = self.sbar_metrics() else {
            return;
        };
        let span = library::sbar_span(top, bottom, max + (bottom - top));
        if span <= 0 {
            return;
        }
        let want = start_scroll + (dy as i64 * max as i64 / span as i64) as i32;
        let delta = want.clamp(0, max) - self.sbar_scroll();
        self.scroll_px(delta);
    }

    pub fn sbar_release(&mut self) {
        self.sbar = None;
        self.sbar_az = None;
    }

    /// Is a scrollbar drag in progress? The renderer widens and accents the thumb while it is, so
    /// the (much wider) grab zone gives some feedback that it took the gesture.
    pub fn sbar_active(&self) -> bool {
        self.sbar.is_some()
    }

    pub fn swipe(&mut self, dir: i32, x: i32, y: i32) -> Vec<Action> {
        if self.locked || self.overlay_open() {
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
            // Turned on its side, a swipe means nothing the window has: Winamp has no gestures.
            Screen::NowPlaying if self.tape_view() => vec![],
            // A swipe on the playlist pages through the queue, a page a swipe, like turning a page:
            // leftward is the next one. Anywhere else it means nothing.
            Screen::NowPlaying if self.winamp_view() => {
                let ly = crate::winamp::to_land(x, y).1;
                if crate::winamp::on_playlist(ly, self.wa_playlist) && !self.context.is_empty() {
                    let page = crate::winamp::pl_rows(self.wa_playlist)
                        .saturating_sub(1)
                        .max(1);
                    let last = self.context.len() - 1;
                    let first = self.wa_pl_first();
                    self.wa_pl_first = Some(if dir < 0 {
                        (first + page).min(last)
                    } else {
                        first.saturating_sub(page)
                    });
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
                let l = self.np_layout();
                if (l.page_top..l.page_bot).contains(&y) {
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
            Screen::Library if dir < 0 => match self.lib_tab {
                Tab::Songs => {
                    let song = library::hit_row_at(
                        self.lib_tab,
                        &self.lib,
                        self.lib_scroll_px,
                        y,
                        self.lib_band(),
                    )
                    .and_then(|rank| library::song_at(&self.lib, self.lib_sort, rank))
                    .cloned();
                    match song {
                        Some(s) => self.enqueue_at(s, y, QueueAt::Next),
                        None => vec![],
                    }
                }
                Tab::Albums => {
                    if let Some(s) = self.albums_track_at(y) {
                        return self.enqueue_at(s, y, QueueAt::Next);
                    }
                    if let Some(al) = self.albums_album_at(y) {
                        return self.enqueue_album_at(&al, y, QueueAt::Next);
                    }
                    vec![]
                }
                _ => vec![],
            },
            Screen::Album if dir < 0 => {
                let song = self.lib.albums_flat().get(self.album_view).and_then(|al| {
                    library::album_hit_track(al, self.album_scroll_px, y)
                        .and_then(|ti| al.row_file(ti).map(|(s, _)| s.clone()))
                });
                match song {
                    Some(s) => self.enqueue_at(s, y, QueueAt::Next),
                    None => vec![],
                }
            }
            // On the queue itself, either direction removes the row — there is nothing to queue.
            Screen::UpNext => match self.up_next_layout().at(y, self.queue_scroll_px) {
                Some(crate::up_next::Slot::Upcoming(i)) => self.remove_upcoming(i),
                _ => vec![],
            },
            // Search results, folder files and SensMe channel tracks queue like every other track
            // list: the same two gestures on every row of music in the app.
            Screen::Search | Screen::Folders | Screen::SensMe => match self.list_song_at(y) {
                Some(s) => {
                    let at = if dir < 0 {
                        QueueAt::Next
                    } else {
                        QueueAt::Later
                    };
                    self.enqueue_at(s, y, at)
                }
                None => vec![],
            },
            // The playlist page's rows queue exactly like every other track list.
            Screen::Playlist => match self.playlist_track_at(y) {
                Some(s) => {
                    let at = if dir < 0 {
                        QueueAt::Next
                    } else {
                        QueueAt::Later
                    };
                    self.enqueue_at(s, y, at)
                }
                None => vec![],
            },
            // The artist page's track and album rows queue.
            Screen::Artist => {
                let at = if dir < 0 {
                    QueueAt::Next
                } else {
                    QueueAt::Later
                };
                if let Some(s) = self.artist_track_at(y) {
                    return self.enqueue_at(s, y, at);
                }
                if let Some(al) = self.artist_album_at(y) {
                    return self.enqueue_album_at(&al, y, at);
                }
                vec![]
            }
            Screen::Library if dir > 0 => match self.lib_tab {
                Tab::Songs => {
                    let song = library::hit_row_at(
                        self.lib_tab,
                        &self.lib,
                        self.lib_scroll_px,
                        y,
                        self.lib_band(),
                    )
                    .and_then(|rank| library::song_at(&self.lib, self.lib_sort, rank))
                    .cloned();
                    match song {
                        Some(s) => self.enqueue_at(s, y, QueueAt::Later),
                        None => vec![],
                    }
                }
                Tab::Albums => {
                    if let Some(s) = self.albums_track_at(y) {
                        return self.enqueue_at(s, y, QueueAt::Later);
                    }
                    if let Some(al) = self.albums_album_at(y) {
                        return self.enqueue_album_at(&al, y, QueueAt::Later);
                    }
                    vec![]
                }
                _ => vec![],
            },
            Screen::Album if dir > 0 => {
                // Right-swipe a track row inside an album drill-in → queue it.
                //
                // RETURNS THE ACTION. It used to call `enqueue` — which throws the returned
                // `Vec<Action>` away — and then return `vec![]`, so this one gesture put the track
                // in the UI's queue, popped the "Added to queue" toast, and never told the shell.
                // Nothing re-issued the sequence, so the track sat in Up Next and did not play
                // where it said it would; it only took effect if some LATER queue edit happened to
                // flush. The mirror gesture one arm up (`dir < 0`, Play Next) always returned it.
                let song = self.lib.albums_flat().get(self.album_view).and_then(|al| {
                    library::album_hit_track(al, self.album_scroll_px, y)
                        .and_then(|ti| al.row_file(ti).map(|(s, _)| s.clone()))
                });
                match song {
                    Some(s) => self.enqueue_at(s, y, QueueAt::Later),
                    None => vec![],
                }
            }
            _ => vec![],
        }
    }

    /// Add an album's tracks to Up Next: straight after the playing track, or at the very end.
    fn enqueue_album_at(&mut self, album: &AlbumRow, y: i32, at: QueueAt) -> Vec<Action> {
        if album.track_list.is_empty() {
            return vec![];
        }
        match at {
            QueueAt::Next => {
                self.notify_format("Playing next — {}", vec![("0", album.name.clone())])
            }
            QueueAt::Later => self.notify_format(
                "Album added to Up Next — {}",
                vec![("0", album.name.clone())],
            ),
        }
        self.queue_anim_y = y;
        self.queue_anim_frames = QUEUE_ANIM_FRAMES;
        self.insert_rows(album.track_list.clone(), at)
    }

    /// Add a track to Up Next: straight after the playing track, or at the very end.
    ///
    /// The toast names WHICH it was, because the two gestures are mirror images and the only way to
    /// know a left-swipe registered as "next" rather than "later" is to be told.
    fn enqueue_at(&mut self, s: SongRow, y: i32, at: QueueAt) -> Vec<Action> {
        match at {
            QueueAt::Next => self.notify_format("Playing next — {}", vec![("0", s.title.clone())]),
            QueueAt::Later => {
                self.notify_format("Added to Up Next — {}", vec![("0", s.title.clone())])
            }
        }
        self.queue_anim_y = y;
        self.queue_anim_frames = QUEUE_ANIM_FRAMES;
        self.insert_rows(vec![s], at)
    }

    /// Put rows into the sequence. `Next` = straight after the playing track, ahead of everything
    /// else still to come; `Later` = after the last track in the list, so an album queued while
    /// another plays starts when that one FINISHES — the thing the old two-list queue could not do.
    fn insert_rows(&mut self, rows: Vec<SongRow>, at: QueueAt) -> Vec<Action> {
        if rows.is_empty() {
            return vec![];
        }
        self.hand_added = true;
        self.added_ids.extend(rows.iter().map(|r| r.object_id));
        if self.context.is_empty() {
            // Nothing has played yet: the list starts here. It does not start PLAYING — adding to
            // a queue is not a transport control — but the next ▶ plays it (cinder-ffi arms that
            // on `QueueChanged` when nothing has been played).
            self.context = rows;
            self.context_idx = 0;
            self.pre_shuffle = None;
            self.queue_follow = true;
            return vec![Action::QueueChanged];
        }
        let ids: Vec<i64> = rows.iter().map(|r| r.object_id).collect();
        match at {
            QueueAt::Next => {
                self.pre_shuffle_insert_next(&ids);
                let pos = (self.context_idx + 1).min(self.context.len());
                self.context.splice(pos..pos, rows);
            }
            QueueAt::Later => {
                self.context.extend(rows);
                if let Some(pre) = self.pre_shuffle.as_mut() {
                    pre.extend(ids);
                }
            }
        }
        vec![Action::QueueChanged]
    }

    /// Record rows added straight after the playing track in the un-shuffle order too, at the
    /// same place relative to it — otherwise turning shuffle off would put them at the end.
    fn pre_shuffle_insert_next(&mut self, ids: &[i64]) {
        let playing = self.context.get(self.context_idx).map(|t| t.object_id);
        if let Some(pre) = self.pre_shuffle.as_mut() {
            let at = playing
                .and_then(|id| pre.iter().position(|p| *p == id))
                .map_or(pre.len(), |i| i + 1);
            pre.splice(at..at, ids.iter().copied());
        }
    }

    /// How many tracks are still to come after the playing one.
    pub fn upcoming_len(&self) -> usize {
        if self.context.is_empty() {
            0
        } else {
            self.context.len() - (self.context_idx + 1).min(self.context.len())
        }
    }

    /// Take one upcoming row out of the list — the swipe on an Up Next row. `i` is a SEQUENCE
    /// index and must be after the playing track: the playing row is not removable, and the rows
    /// before it are not on screen.
    fn remove_upcoming(&mut self, i: usize) -> Vec<Action> {
        if i <= self.context_idx || i >= self.context.len() {
            return vec![];
        }
        // Which copy of its song this row is: `unshuffle_context` pairs the k-th copy in the list
        // with the k-th in the saved order, so that is the one to drop, not the first.
        let k = self.context[..i]
            .iter()
            .filter(|r| r.object_id == self.context[i].object_id)
            .count();
        let gone = self.context.remove(i);
        if let Some(pre) = self.pre_shuffle.as_mut() {
            let at = pre
                .iter()
                .enumerate()
                .filter(|(_, p)| **p == gone.object_id)
                .nth(k)
                .map(|(j, _)| j);
            if let Some(j) = at {
                pre.remove(j);
            }
        }
        self.notify_format("Removed — {}", vec![("0", gone.title)]);
        // The list just got shorter, so the scroll may now be past its end.
        self.queue_scroll_px = self
            .queue_scroll_px
            .min(self.up_next_layout().max_scroll_px());
        vec![Action::QueueChanged]
    }

    /// Push the track that is playing RIGHT NOW onto the history, because `incoming` is about to
    /// replace it. Call before the index moves — see `track_started`.
    ///
    /// The two guards are the whole policy: a track does not follow itself (repeat-one, or the
    /// shell re-reporting the same URI), and the list is capped at [`HISTORY_MAX`] from the front.
    fn history_push(&mut self, incoming: i64) {
        let Some(outgoing) = self.playing_row().cloned() else {
            return;
        };
        if outgoing.object_id == incoming {
            return; // the same song again — one row, not two
        }
        if self.history.last().map(|h| h.object_id) == Some(outgoing.object_id) {
            return; // already the newest entry; a re-report is not a second listen
        }
        self.history.push(outgoing);
        if self.history.len() > HISTORY_MAX {
            // Drop from the FRONT: the oldest listens are the ones worth losing, and `drain` does
            // it in one move rather than one `remove(0)` memmove per track for the rest of time.
            let excess = self.history.len() - HISTORY_MAX;
            self.history.drain(..excess);
        }
    }

    /// The row actually playing. The `SongRow` twin of `playing_object_id`.
    fn playing_row(&self) -> Option<&SongRow> {
        self.context.get(self.context_idx)
    }

    /// Up Next's PREVIOUSLY PLAYED, oldest first.
    pub fn history(&self) -> &[SongRow] {
        &self.history
    }

    /// Empty the history. The CLEAR on the PREVIOUSLY PLAYED header, and the only thing that does
    /// this — see [`App::history`] for why it is not shared with the list's CLEAR chip.
    pub fn history_clear(&mut self) -> Vec<Action> {
        if self.history.is_empty() {
            return vec![];
        }
        self.history.clear();
        // The list above the fold just got shorter, so a scroll offset measured against the old
        // content height would leave the screen showing blank space below the last row.
        self.queue_scroll_px = self
            .queue_scroll_px
            .min(self.up_next_layout().max_scroll_px());
        self.row_drag = None;
        // No Action: the history is Cinder's own record and nothing about the transport changes.
        vec![]
    }

    /// Drop everything still to come — the CLEAR chip's confirmed answer. The playing track
    /// finishes and playback stops there, unless something is added in the meantime.
    pub fn queue_clear(&mut self) -> Vec<Action> {
        if self.upcoming_len() == 0 {
            return vec![];
        }
        self.context.truncate(self.context_idx + 1);
        // One track left has no order to go back to.
        self.pre_shuffle = None;
        self.hand_added = false;
        self.added_ids.clear();
        self.queue_scroll_px = self
            .queue_scroll_px
            .min(self.up_next_layout().max_scroll_px());
        self.row_drag = None;
        vec![Action::QueueChanged]
    }

    /// Set the PLAY SEQUENCE — what is now playing, and which track of it started.
    ///
    /// Called by the shell every time it resolves a play action: a track tap, an album, a playlist,
    /// any Shuffle band. It REPLACES the list, unless the "keep Up Next" answer asked for what was
    /// still to come to be kept — then that plays after the new sequence.
    pub fn set_play_context(&mut self, rows: Vec<SongRow>, start: usize) {
        let kept = if self.queue_keep && !self.context.is_empty() {
            let from = (self.context_idx + 1).min(self.context.len());
            self.context.split_off(from)
        } else {
            Vec::new()
        };
        self.queue_keep = false;
        // The kept rows are still the user's own list, so the next replace still asks.
        self.hand_added = !kept.is_empty();
        self.added_ids = kept.iter().map(|r| r.object_id).collect();
        self.context_idx = start.min(rows.len().saturating_sub(1));
        self.context = rows;
        self.context.extend(kept);
        // A new sequence has nothing to un-shuffle back to. Holding the old order would let a
        // later shuffle-off reorder THIS sequence into some previous album's order.
        self.pre_shuffle = None;
        self.queue_scroll_px = 0;
        self.row_drag = None;
        // RE-ARM THE FOLLOW. Every play that does not start from the Up Next screen itself arrives
        // here — a library tap, an album, a playlist, a Shuffle band. Without this, a user who had
        // scrolled Up Next once (which hands the list to them, deliberately) would find it stuck at
        // the top for the rest of the session while a completely different album played.
        self.queue_follow = true;
        self.up_next_cur = None;
        self.up_next_id = None;
    }

    /// What `set_play_context` would forget that a jump WITHIN the list must keep: the un-shuffle
    /// order and whether the list was built by hand. The shell reads it before re-issuing the same
    /// list from another row (`Action::PlayContextAt`) and hands it back afterwards.
    pub fn list_state(&self) -> (Option<Vec<i64>>, bool, Vec<i64>) {
        (
            self.pre_shuffle.clone(),
            self.hand_added,
            self.added_ids.clone(),
        )
    }

    /// Put `list_state` back. The un-shuffle order is kept only if it still describes the list —
    /// a row whose file has gone drops out of the re-issued list, and a stale order would put the
    /// rest back in the wrong places.
    pub fn restore_list_state(&mut self, (pre, hand, added): (Option<Vec<i64>>, bool, Vec<i64>)) {
        self.pre_shuffle = pre.filter(|p| p.len() == self.context.len());
        self.hand_added = hand;
        self.added_ids = added;
    }

    /// Is a song the user added still waiting in Up Next? Once every added song has played, what
    /// is left is the album or playlist they started, which is not worth a question.
    fn added_still_to_come(&self) -> bool {
        let rest = self.context.get(self.context_idx + 1..).unwrap_or(&[]);
        if self.added_ids.is_empty() {
            return !rest.is_empty();
        }
        rest.iter().any(|r| self.added_ids.contains(&r.object_id))
    }

    /// The play sequence and our position in it. The shell reads these to build what it hands
    /// PlayerService: `context[idx..]`.
    pub fn context(&self) -> &[SongRow] {
        &self.context
    }
    pub fn context_idx(&self) -> usize {
        self.context_idx
    }

    /// Select an exact queue occurrence for ZAPPIN's full-play handoff. Object ids may repeat in
    /// a hand-built queue, so identity is the queue position plus generation, not object id alone.
    pub fn set_context_index(&mut self, index: usize) -> bool {
        if index >= self.context.len() {
            return false;
        }
        self.context_idx = index;
        true
    }

    /// Point the index at a track by id, without recording anything. Returns true if that moved
    /// it. Searches BACKWARDS from the playing row first — its one caller outside the tests is ◁,
    /// and a track in the list twice should land on the copy just behind the playing one — then
    /// the whole list. An id that is not in the list leaves the index alone rather than guessing.
    pub fn set_context_playing(&mut self, object_id: i64) -> bool {
        let idx = self.context_idx.min(self.context.len());
        let pos = self.context[..idx]
            .iter()
            .rposition(|t| t.object_id == object_id)
            .or_else(|| self.context.iter().position(|t| t.object_id == object_id));
        match pos {
            Some(i) if i != self.context_idx => {
                self.context_idx = i;
                true
            }
            _ => false,
        }
    }

    /// ◁ is about to replay `row` and then the playing track. Make the list say so: step back onto
    /// it if it is in the list, or put it in front of the playing track if it is not (it came from
    /// an album that has since been replaced). Otherwise Up Next would name the wrong song as
    /// playing for the whole of the replay.
    pub fn rewind_to(&mut self, row: SongRow) {
        if self.context.is_empty() {
            return;
        }
        if !self.set_context_playing(row.object_id)
            && self.context.get(self.context_idx).map(|t| t.object_id) != Some(row.object_id)
        {
            let at = self.context_idx.min(self.context.len());
            if let Some(pre) = self.pre_shuffle.as_mut() {
                let playing = self.context.get(at).map(|t| t.object_id);
                let j = playing
                    .and_then(|id| pre.iter().position(|p| *p == id))
                    .unwrap_or(pre.len());
                pre.insert(j, row.object_id);
            }
            self.context.insert(at, row);
        }
    }

    /// A track has STARTED; move the index onto it and record the one it replaced.
    ///
    /// `stale` = a list edit has not reached PlayerService yet, so the service chose this track
    /// from the sequence it held BEFORE the edit. Returns true when that happened and the list has
    /// been put right: the index is on the track the LIST says comes next, and the caller must hand
    /// PlayerService `context[idx..]` from position 0. The track that started is not lost — it is
    /// still in the list, after the rows that were put in front of it.
    ///
    /// The rules, in order:
    ///   * the playing track again (repeat-one, a re-report) — nothing moves;
    ///   * the next row — ordinary progression, one step, never a search;
    ///   * `stale` and not a step backwards — PlayerService is behind the list (a Play next that
    ///     has not been issued, a removed row); take the list's next row instead. Reported
    ///     2026-09-11 against the old queue as "it plays the next in the album, then the queue";
    ///   * anything else is a deliberate jump (◁, an Up Next tap) — search forward from here
    ///     first, then from the top, so a track in the list twice lands on the NEXT copy.
    ///
    /// Repeat album: the run of the list the playing song's album occupies, as `start..end`
    /// (`end` exclusive) — the rows either side of `context_idx` from the same album. `None` with
    /// nothing playing.
    ///
    /// A RUN, not "every row of that album": Up Next is one list, and an album queued twice, or
    /// split by a song queued into the middle of it, is two places in it. The lap is the one you
    /// are in. Same album = the same `album_id`, or the same album name (`art`) where the id is
    /// unknown (0). With shuffle on the album's songs are scattered, so the run is usually the one
    /// song — which is what the shell falls back to (it only laps a run of two or more).
    pub fn album_run(&self) -> Option<(usize, usize)> {
        let idx = self.context_idx;
        let cur = self.context.get(idx)?;
        let same = |row: &SongRow| {
            if cur.album_id != 0 && row.album_id != 0 {
                row.album_id == cur.album_id
            } else {
                !cur.art.is_empty() && row.art == cur.art
            }
        };
        let start = self.context[..idx]
            .iter()
            .rposition(|r| !same(r))
            .map_or(0, |p| p + 1);
        let end = self.context[idx + 1..]
            .iter()
            .position(|r| !same(r))
            .map_or(self.context.len(), |p| idx + 1 + p);
        Some((start, end))
    }

    pub fn track_started(&mut self, object_id: i64, stale: bool) -> bool {
        let idx = self.context_idx;
        let at = |a: &Self, i: usize| a.context.get(i).map(|t| t.object_id);
        if at(self, idx) == Some(object_id) {
            return false;
        }
        let next = idx + 1;
        if at(self, next) == Some(object_id) {
            self.history_push(object_id);
            self.context_idx = next;
            return false;
        }
        let behind = self
            .context
            .iter()
            .take(idx)
            .any(|t| t.object_id == object_id);
        if stale && next < self.context.len() && !behind {
            let incoming = self.context[next].object_id;
            self.history_push(incoming);
            self.context_idx = next;
            return true;
        }
        let pos = self
            .context
            .iter()
            .skip(next)
            .position(|t| t.object_id == object_id)
            .map(|p| p + next)
            .or_else(|| self.context.iter().position(|t| t.object_id == object_id));
        self.history_push(object_id);
        if let Some(p) = pos {
            self.context_idx = p;
        }
        false
    }

    /// SHUFFLE: the playing track stays playing and moves to the top; EVERY other track in the
    /// list is dealt in a random order behind it.
    ///
    /// Every other track, not just the ones after the playing one. Until 2026-09-23 this shuffled
    /// only `context[idx+1..]`, so starting an album at track 7 and pressing shuffle played a
    /// random order of tracks 8-12 and never tracks 1-6 — "it cuts out half of the songs". Stock
    /// and every other player shuffle the whole album. Turning shuffle off (`unshuffle_context`)
    /// puts the original order back and continues from the playing track.
    ///
    /// Returns the action that re-issues the sequence, or nothing when there is too little to
    /// shuffle.
    pub fn queue_shuffle(&mut self) -> Vec<Action> {
        if self.context.len() < 3 {
            self.notify(crate::i18n::text("Nothing to shuffle"));
            return vec![];
        }
        // REMEMBER THE ORDER BEFORE THE FIRST SHUFFLE TOUCHES IT, so shuffle-off has something to
        // restore. Recorded here rather than at the toggle because this is the only place that
        // permutes the sequence, and the MIX chip reaches it too.
        if self.pre_shuffle.is_none() {
            self.pre_shuffle = Some(self.context.iter().map(|t| t.object_id).collect());
        }
        let cur = self.context_idx.min(self.context.len() - 1);
        let ids: Vec<i64> = self.context.iter().map(|t| t.object_id).collect();
        let glue = self.glue_mask(&ids);
        if glue.contains(&true) {
            return self.queue_shuffle_glued(cur, &glue);
        }
        let playing = self.context.remove(cur);
        self.context.insert(0, playing);
        self.context_idx = 0;
        // By songs, albums or artists (Settings ▸ Shuffle), the playing track first and, when
        // grouped, its own group carrying on behind it. `shuffle::deal` says how.
        let rows: Vec<crate::shuffle::Row> = self
            .context
            .iter()
            .map(|t| crate::shuffle::Row {
                album: crate::shuffle::album_key(t.album_id, t.object_id),
                artist: crate::shuffle::artist_key(t.group_artist()),
                disc: t.disc,
                track: t.track,
            })
            .collect();
        let (order, by) = crate::shuffle::deal(&rows, self.shuffle_by, Some(0), &mut self.rng);
        // WHAT HAS PLAYED GOES LAST, still dealt, so nothing is cut and nothing heard comes
        // straight back. Rows 1..=cur were before the playing one. The playing song's own album
        // (or artist) is the exception in a grouped deal: it comes round to its start first, as
        // `deal` put it. A stable split, so every group stays whole and in order.
        let key = |r: &crate::shuffle::Row| match by {
            crate::shuffle::ShuffleBy::Songs | crate::shuffle::ShuffleBy::Spread => None,
            crate::shuffle::ShuffleBy::Albums => Some(r.album),
            crate::shuffle::ShuffleBy::Artists => Some(r.artist),
        };
        let own = key(&rows[0]);
        let (mut order, heard): (Vec<usize>, Vec<usize>) = order
            .into_iter()
            .partition(|&i| i == 0 || i > cur || (own.is_some() && key(&rows[i]) == own));
        order.extend(heard);
        self.context = crate::shuffle::apply(std::mem::take(&mut self.context), &order);
        self.notify(by.toast());
        vec![Action::QueueChanged]
    }

    /// `queue_shuffle` for a list with glued runs (plan 1c). The playing song is NOT moved to the
    /// top first — that would cut it off from its run — but dealt as the lead, and the rows before
    /// it go last as heard, exactly as the plain path does, except that a run is never split.
    fn queue_shuffle_glued(&mut self, cur: usize, glue: &[bool]) -> Vec<Action> {
        let rows: Vec<crate::shuffle::Row> = self
            .context
            .iter()
            .map(|t| crate::shuffle::Row {
                album: crate::shuffle::album_key(t.album_id, t.object_id),
                artist: crate::shuffle::artist_key(t.group_artist()),
                disc: t.disc,
                track: t.track,
            })
            .collect();
        let (order, by) =
            crate::shuffle::deal_glued(&rows, glue, self.shuffle_by, Some(cur), &mut self.rng);
        // As the plain path: rows before the playing one were heard and go last, except its own
        // album (or artist) in a grouped deal. Judged by each row's RUN HEAD, so no run is split;
        // only the playing run's own heard rows leave it, which is what heard means.
        let head: Vec<usize> = (0..rows.len())
            .scan(0, |h, i| {
                if !glue.get(i).copied().unwrap_or(false) {
                    *h = i;
                }
                Some(*h)
            })
            .collect();
        let key = |r: &crate::shuffle::Row| match by {
            crate::shuffle::ShuffleBy::Songs | crate::shuffle::ShuffleBy::Spread => None,
            crate::shuffle::ShuffleBy::Albums => Some(r.album),
            crate::shuffle::ShuffleBy::Artists => Some(r.artist),
        };
        let own = key(&rows[head[cur]]);
        let (mut order, heard): (Vec<usize>, Vec<usize>) = order.into_iter().partition(|&i| {
            i >= cur || (head[i] != head[cur] && own.is_some() && key(&rows[head[i]]) == own)
        });
        order.extend(heard);
        self.context = crate::shuffle::apply(std::mem::take(&mut self.context), &order);
        self.context_idx = 0;
        self.notify(by.toast());
        vec![Action::QueueChanged]
    }

    /// Put the sequence back in the order it had before the first shuffle. Returns the action that
    /// re-issues it, or nothing when there was no shuffle to undo.
    ///
    /// The CURRENT TRACK KEEPS PLAYING and the index follows it to wherever it sits in the restored
    /// order — turning shuffle off means "play the rest of this in order from here", not "start the
    /// album again".
    ///
    /// A track can be in the list twice (an album queued while it plays), so ranks are handed out
    /// per OCCURRENCE: the first copy of an id takes its first recorded position, the second copy
    /// the second. Ranking by id alone gave both copies one rank and stacked them side by side.
    /// Rows the recorded order does not mention keep their relative order at the end.
    pub fn unshuffle_context(&mut self) -> Vec<Action> {
        let Some(order) = self.pre_shuffle.take() else {
            return vec![];
        };
        if self.context.len() < 2 {
            return vec![];
        }
        let mut slots: std::collections::HashMap<i64, std::collections::VecDeque<usize>> =
            std::collections::HashMap::new();
        for (i, id) in order.iter().enumerate() {
            slots.entry(*id).or_default().push_back(i);
        }
        let end = order.len();
        let playing = self.context_idx;
        let mut ranked: Vec<(usize, usize, SongRow)> = std::mem::take(&mut self.context)
            .into_iter()
            .enumerate()
            .map(|(i, t)| {
                let r = slots
                    .get_mut(&t.object_id)
                    .and_then(|q| q.pop_front())
                    .unwrap_or(end + i);
                (r, i, t)
            })
            .collect();
        ranked.sort_by_key(|(r, _, _)| *r);
        // Follow the ROW that is audible — not the old index, which after a reorder points at an
        // unrelated song, and not the first copy of its id, which may be a different occurrence.
        self.context_idx = ranked
            .iter()
            .position(|(_, i, _)| *i == playing)
            .unwrap_or(0);
        self.context = ranked.into_iter().map(|(_, _, t)| t).collect();
        self.notify(crate::i18n::text("Playing in order"));
        vec![Action::QueueChanged]
    }

    fn go(&mut self, s: Screen) {
        if s == Screen::Settings {
            self.palettes_stale = true; // see `push`
        }
        // Remember the browsing you leave for Now Playing, so Back there returns to it. Not when
        // Now Playing is already on top, and never a stack holding the Lock screen, onboarding or
        // USB storage: Back must not lead to a screen you only pass through.
        if s == Screen::NowPlaying {
            let from = std::mem::take(&mut self.stack);
            if from.last().is_some_and(|x| *x != Screen::NowPlaying)
                && !from
                    .iter()
                    .any(|x| matches!(x, Screen::Lock | Screen::Onboarding | Screen::UsbStorage))
            {
                self.np_from = from;
            }
        }
        self.stack = vec![s];
    }

    /// Hand the shell-decoded 96x96 cover for the open album to the UI (None = draw the gradient).
    pub fn set_album_cover(&mut self, img: Option<crate::art::Image>) {
        self.album_cover = img;
    }

    /// Album ids the Library grid wants 96 px covers for, nearest the screen first. Empty unless
    /// the Library is on screen in a grid. The shell polls this like `open_album_id`, loads what is
    /// missing into the library with `put_grid_cover`, and drops the rest with `keep_grid_covers`.
    pub fn grid_cover_wants(&self) -> Vec<i64> {
        if self.current() != Screen::Library {
            return Vec::new();
        }
        library::grid_cover_ids(self.lib_tab, &self.lib, self.album_sort, self.lib_scroll_px)
    }

    pub fn has_grid_cover(&self, album_id: i64) -> bool {
        self.lib.covers.contains_key(&album_id)
    }

    pub fn put_grid_cover(&mut self, album_id: i64, img: crate::art::Image) {
        self.lib.covers.insert(album_id, img);
    }

    /// Drop every grid cover not in `keep`.
    pub fn keep_grid_covers(&mut self, keep: &[i64]) {
        self.lib.covers.retain(|id, _| keep.contains(id));
    }

    /// `album_id` of the album currently drilled into, if any. The shell polls this to know which
    /// cover to load out of its art cache — the UI never reads the cache itself.
    pub fn open_album_id(&self) -> Option<i64> {
        if self.current() != Screen::Album {
            return None;
        }
        self.lib
            .albums_flat()
            .get(self.album_view)
            .map(|a| a.album_id)
    }

    /// Screens that carry the Now Playing return bar: the library browse list and the album
    /// drill-in. These are the places you end up several pushes deep from Now Playing, which is
    /// exactly where Back-ing out one screen at a time is tedious.
    /// UP NEXT IS ON THIS LIST BECAUSE IT ALREADY PAYS FOR THE SPACE. `up_next::LIST_BOTTOM` is
    /// derived from `chrome::NP_BAR_H`, so the screen has always reserved the bottom 64 px — it
    /// simply never drew anything there, which showed up as a black bar under the queue (reported
    /// 2026-09-05).
    ///
    /// The alternative was to give Up Next the full height instead. That is the wrong repair: the
    /// scrollbar Up Next draws is `library::scrollbar`, and `sbar_begin` measures the thumb's
    /// travel against the LIBRARY's bottom — the comment on `up_next::LIST_BOTTOM` exists to keep
    /// those two from drifting apart. Expanding the list would have silently mis-scaled the thumb
    /// on the one screen where the scrollbar is also a drag target.
    ///
    /// Drawing the bar also gives Up Next the same play/pause zone and the same tap-to-return that
    /// every other browsing screen has, which is what the space was reserved for in the first place.
    ///
    /// AND FOLDERS, FOR THE IDENTICAL REASON, found by the 2026-09-06 UI audit. `folders.rs` uses
    /// `library::LIST_BOTTOM` for its clip band, its hit test and its scroll extent — so it has
    /// always reserved the same bottom 64 px and drawn nothing in them. It is the same defect that
    /// was reported against Up Next on 2026-09-05, on the screen next door, left standing because
    /// the fix was applied to the reported screen rather than to the rule.
    ///
    /// The rule is now stated as one: A SCREEN THAT RESERVES THE BAR'S SPACE MUST DRAW THE BAR.
    /// `every_screen_that_reserves_the_np_bar_draws_it` holds every screen whose list stops at
    /// `library::list_bottom()` to it, so the next one cannot be added without the bar.
    fn shows_np_bar(s: Screen) -> bool {
        matches!(
            s,
            Screen::Library
                | Screen::Album
                | Screen::Artist
                | Screen::Playlist
                | Screen::UpNext
                | Screen::Folders
                | Screen::SensMe
                | Screen::Books
        )
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

    /// The A–Z rail's "which letters exist" map, recomputed only when it can have changed.
    ///
    /// `az_present` walks every row in the library to fill 27 booleans — 65 us on the host at the
    /// device's library size, on EVERY painted frame, which is about a quarter of a Library frame
    /// once the art cache is warm and the rows are cheap blits. The answer depends on the library
    /// and the two sort chips and nothing else, so it is cached against exactly those.
    ///
    /// Invalidated by `set_library`, which is the only way the library is ever replaced.
    fn az_present_memo(&mut self) -> [bool; 27] {
        let key = (self.lib_tab as u8, self.lib_sort, self.album_sort);
        if let Some((k, v)) = self.az_memo {
            if k == key {
                return v;
            }
        }
        let v = library::az_present(self.lib_tab, &self.lib, self.lib_sort, self.album_sort);
        self.az_memo = Some((key, v));
        v
    }

    /// Label state for Settings ▸ Database. See the `rescanning` field for why it has two exits.
    pub fn set_rescanning(&mut self, on: bool) {
        self.rescanning = on;
    }
    pub fn rescanning(&self) -> bool {
        self.rescanning
    }
    /// Is the player working on something that takes seconds and can fail? Drives the ring in the
    /// status bar, so a scan or a connect can be seen from every screen, not only the one that
    /// started it (the owner, 11-10).
    pub fn busy(&self) -> bool {
        use crate::wifi::State as W;
        self.rescanning
            || self.bt_scanning
            || self.bt_connecting.is_some()
            || self.bt_connecting_name.is_some()
            || self.speakers_scanning
            || self.wifi_scanning
            || matches!(self.wifi_status.state, W::Busy | W::Connecting)
            || self.radio_status.state == crate::netradio::State::Connecting
    }
    /// What Settings ▸ Database shows: progress while a scan is outstanding, otherwise the size of
    /// the library it would rescan — which is the number people open that row to check.
    pub fn database_label(&self) -> String {
        if self.rescanning {
            return crate::i18n::text("Rescanning…").to_string();
        }
        let n = self.library().songs.len();
        if n == 0 {
            return crate::i18n::text("Empty").to_string();
        }
        crate::ui_format!("{n} tracks", n = n)
    }

    pub fn set_library(&mut self, mut lib: Library) {
        // A reloaded library is the scan's answer arriving — see the `rescanning` field.
        self.rescanning = false;
        // The shell builds every library in plain order; the artist lists are put in the user's
        // order here, so a reload can never quietly drop the Ignore "The" setting.
        lib.ignore_the = self.ignore_the;
        lib.views = self.lib_views;
        lib.folders_tab = self.opt(Opt::FoldersTab);
        lib.prepare_order();
        // Photos arrive from a background load after the library; keep the last ones until then.
        if lib.artist_photos.is_empty() {
            lib.artist_photos = std::mem::take(&mut self.lib.artist_photos);
        }
        self.lib = lib;
        self.az_memo = None; // a new library is a new set of letters
        self.lib_idx = 0;
        self.lib_scroll_px = 0;
        self.fling_v = 0.0;
        self.album_expanded = None;
        // The open SensMe channel is an INDEX into the library that just went away. Left set, a
        // reload (a rescan finishing, a card going in) would leave the channel page open on a
        // channel that no longer exists — empty, titled "SensMe", with no sign of what happened.
        self.sensme_channel = None;
        self.sensme_scroll_px = 0;
        self.sensme_scroll_saved = 0;
        // The smart playlists are computed from the library that just arrived.
        self.rebuild_smart(None);
        // "Last screen" was waiting for this: the place it names may be an album or a playlist,
        // and those only resolve against a library. Still only if nothing has moved since boot.
        if std::mem::take(&mut self.home_pending) && self.stack == [Screen::NowPlaying] {
            if let Some(p) = self.home_last.clone() {
                self.restore_pin(&p);
            }
        }
    }

    /// Keep the library cursor's row fully inside the pixel-scrolled window (button nav).
    fn lib_ensure_visible(&mut self) {
        let row_top = library::row_top_px(
            self.lib_tab,
            &self.lib,
            self.lib_idx,
            self.album_sort,
            self.album_expanded,
        );
        // One line of the tab in its current view (an album row, a compact row, a grid line).
        let rh = library::cursor_h(self.lib_tab, &self.lib);
        let view = library::view_h(self.lib_tab);
        if row_top < self.lib_scroll_px {
            self.lib_scroll_px = row_top;
        } else if row_top + rh > self.lib_scroll_px + view {
            self.lib_scroll_px = row_top + rh - view;
        }
    }

    /// Keep the album drill-in cursor's row fully inside its pixel-scrolled window.
    fn album_ensure_visible(&mut self) {
        let row_top = self.album_track_idx as i32 * library::ALBUM_TRACK_RH();
        let view = crate::canvas::H as i32 - 12 - library::album_tracks_top();
        if row_top < self.album_scroll_px {
            self.album_scroll_px = row_top;
        } else if row_top + library::ALBUM_TRACK_RH() > self.album_scroll_px + view {
            self.album_scroll_px = row_top + library::ALBUM_TRACK_RH() - view;
        }
    }

    /// Keep the artist page's track cursor in view. The track rows sit below a variable-height
    /// album block, so the row's content-space y comes from the page layout rather than from
    /// `idx * row_h` — the album rows above it are not the same height.
    /// Keep the playlist cursor on screen when the transport buttons move it.
    fn playlist_ensure_visible(&mut self) {
        let want = self.playlist_track_idx as i32 * library::PLAYLIST_TRACK_RH();
        let Some((max, view)) = self.playlist_row().map(|p| {
            (
                library::playlist_max_scroll_px(p),
                library::playlist_view_h(p),
            )
        }) else {
            return;
        };
        let s = self.playlist_scroll_px;
        let s = if want < s {
            want
        } else if want + library::PLAYLIST_TRACK_RH() > s + view {
            want + library::PLAYLIST_TRACK_RH() - view
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
        let rh = library::ARTIST_TRACK_RH();
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

    /// Handle a button press; returns any actions for the shell to perform.
    pub fn press(&mut self, b: Button) -> Vec<Action> {
        // POWER ALWAYS TOGGLES THE SCREEN, ahead of every modal below. A dialog, USB storage and
        // the intro all swallowed it, so a screen that timed out over one of them could not be
        // woken by Power at all — only the Hold switch, which the shell handles itself. Seen
        // 2026-10-25 on the first stock boots: three presses did nothing, the lock brought it back.
        if b == Button::Power {
            return vec![Action::Sleep];
        }
        // Hold/lock SWITCH engaged: the touchscreen is disabled (see `tap`), but the physical
        // transport + volume buttons still control playback — you can skip/pause/adjust without
        // unlocking. Power only toggles the screen (the shell blanks/wakes the backlight on Sleep);
        // it does NOT unlock. The ONLY thing that unlocks is the Hold switch going off (`set_hold`).
        if self.locked {
            return match b {
                Button::Power => vec![Action::Sleep],
                Button::Play => {
                    self.set_playing_optimistic(!self.playing);
                    vec![Action::PlayPause]
                }
                Button::Right | Button::Next => vec![Action::Next],
                Button::Left | Button::Prev => vec![Action::Prev],
                Button::VolUp => {
                    if self.vol_step(true) {
                        vec![Action::VolUp]
                    } else {
                        vec![]
                    }
                }
                Button::VolDown => {
                    if self.vol_step(false) {
                        vec![Action::VolDown]
                    } else {
                        vec![]
                    }
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
        // The pull-down panel gets the same rules, and Back closes it.
        if self.shelf_open || self.quick_open {
            return match b {
                Button::Back => {
                    self.shelf_open = false;
                    self.quick_open = false;
                    vec![]
                }
                Button::Power => vec![Action::Sleep],
                Button::Play => {
                    self.set_playing_optimistic(!self.playing);
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
                self.set_playing_optimistic(!self.playing);
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
                // Back to where Now Playing was opened from; with nothing to return to, the
                // Library, as the toolbar's Library button does.
                Button::Back => {
                    let from = std::mem::take(&mut self.np_from);
                    if from.is_empty() {
                        self.open_browse();
                    } else {
                        self.stack = from;
                        self.note_browse();
                    }
                    vec![]
                }
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
                    if self.menu_idx + 1 < self.menu_len() {
                        self.menu_idx += 1;
                    }
                    vec![]
                }
                Button::Select | Button::Right => self.activate_menu(self.menu_idx),
                Button::Option
                    if self
                        .menu_visible()
                        .get(self.menu_idx)
                        .is_some_and(|m| m.0 == Screen::Zappin) =>
                {
                    self.push(Screen::Zappin);
                    vec![]
                }
                Button::Back | Button::Left => {
                    self.pop();
                    vec![]
                }
                _ => vec![],
            },
            Screen::Zappin => match b {
                Button::Select => {
                    if self.zappin_active {
                        self.set_playing_optimistic(!self.playing);
                        vec![Action::PlayPause]
                    } else if self.context_idx() < self.context().len() {
                        vec![Action::ZappinStart(self.zappin_preview_ms)]
                    } else {
                        vec![]
                    }
                }
                Button::Left if self.zappin_active => vec![Action::ZappinPrev],
                Button::Right if self.zappin_active => vec![Action::ZappinNext],
                Button::Option => {
                    self.zappin_preview_ms = if self.zappin_preview_ms == 4_000 {
                        15_000
                    } else {
                        4_000
                    };
                    vec![Action::ZappinLength(self.zappin_preview_ms)]
                }
                // Left is Back everywhere else; it only steps between songs while a pass runs.
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
                            library::album_flat_at_rank(&self.lib, self.album_sort, self.lib_idx)
                                .unwrap_or(0);
                        self.album_track_idx = 0;
                        self.album_scroll_px = 0;
                        self.fling_v = 0.0;
                        self.push(Screen::Album);
                        vec![]
                    }
                    // lib_idx is a RANK in the drawn (sorted) order — resolve through it.
                    Tab::Songs => {
                        let ids = self.songs_list_ids();
                        self.start_play_list(ids, self.lib_idx)
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
                    .map(|a| a.rows())
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
                        let album = self
                            .lib
                            .albums_flat()
                            .get(self.album_view)
                            .map(|a| (*a).clone());
                        match album {
                            Some(al) => self.play_album_row(&al, self.album_track_idx),
                            None => vec![],
                        }
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
                        // Same context rule as the tap path: the playlist plays, from here.
                        let idx = self.playlist_track_idx;
                        let hit = self
                            .playlist_row()
                            .filter(|p| idx < p.track_list.len())
                            .map(|p| p.id);
                        hit.map(|id| self.start_play_action(Action::PlayPlaylistAt(id, idx as u32)))
                            .unwrap_or_default()
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
                        self.artist_songs_open = true; // the cursor lives in the SONGS list
                        self.artist_track_idx = self.artist_track_idx.saturating_sub(1);
                        self.artist_ensure_visible();
                        vec![]
                    }
                    Button::Down => {
                        self.artist_songs_open = true;
                        if self.artist_track_idx + 1 < n {
                            self.artist_track_idx += 1;
                            self.artist_ensure_visible();
                        }
                        vec![]
                    }
                    Button::Select => {
                        let ids = self.artist_list_ids();
                        self.start_play_list(ids, self.artist_track_idx)
                    }
                    Button::Back | Button::Left => {
                        self.pop();
                        vec![]
                    }
                    _ => vec![],
                }
            }
            Screen::Settings => match b {
                // The cursor walks the rows of the page on screen, as drawn.
                Button::Up | Button::Down => {
                    let rows = crate::settings::page_rows(self.settings_page, self.settings_lang());
                    let at = rows
                        .iter()
                        .position(|r| *r == self.settings_sel)
                        .unwrap_or(0);
                    let to = if b == Button::Up {
                        at.saturating_sub(1)
                    } else {
                        (at + 1).min(rows.len().saturating_sub(1))
                    };
                    if let Some(r) = rows.get(to) {
                        self.settings_sel = *r;
                    }
                    vec![]
                }
                // Select (or Left/Right) acts on the focused row.
                Button::Select | Button::Right | Button::Left => self.settings_activate(),
                // Back from a topic page is the home page, cursor on the page's link.
                Button::Back if self.settings_page.is_some() => {
                    self.settings_page_back();
                    vec![]
                }
                Button::Back => {
                    self.pop();
                    vec![]
                }
                _ => vec![],
            },
            Screen::Display => match b {
                // Starts and Ends are stepped over unless Night is on Auto.
                Button::Up => {
                    if let Some(r) = (0..self.display_sel)
                        .rev()
                        .find(|&r| crate::display::shown(r))
                    {
                        self.display_sel = r;
                    }
                    vec![]
                }
                Button::Down => {
                    if let Some(r) = (self.display_sel + 1..crate::display::ROWS)
                        .find(|&r| crate::display::shown(r))
                    {
                        self.display_sel = r;
                    }
                    vec![]
                }
                // On the Size slider, Left/Right step the value (Select still steps up).
                Button::Left if self.display_sel == crate::display::ROW_SIZE => {
                    self.step_ui_scale(-1)
                }
                Button::Right if self.display_sel == crate::display::ROW_SIZE => {
                    self.step_ui_scale(1)
                }
                Button::Select | Button::Right | Button::Left => self.display_activate(),
                Button::Back => {
                    self.pop();
                    vec![]
                }
                _ => vec![],
            },
            Screen::UsbDac => match b {
                Button::Select => {
                    self.usb_dac_on = !self.usb_dac_on;
                    self.route_changed();
                    vec![Action::UsbDacToggle(self.usb_dac_on)]
                }
                Button::Back => {
                    self.pop();
                    vec![]
                }
                _ => vec![],
            },
            Screen::Soundscape => self.soundscape_button(b),
            Screen::Presets => match b {
                Button::Back => {
                    self.pop();
                    vec![]
                }
                _ => vec![],
            },
            Screen::DacEq => match b {
                Button::Left => {
                    self.dac_eq_sel = self.dac_eq_sel.saturating_sub(1);
                    vec![]
                }
                Button::Right => {
                    if self.dac_eq_sel + 1 < crate::dac_eq::BANDS {
                        self.dac_eq_sel += 1;
                    }
                    vec![]
                }
                Button::Up => self.dac_eq_nudge(1),
                Button::Down => self.dac_eq_nudge(-1),
                Button::Back => {
                    self.pop();
                    vec![]
                }
                _ => vec![],
            },
            Screen::VolCurve => match b {
                Button::Left => {
                    self.vol_curve_sel = self.vol_curve_sel.saturating_sub(1);
                    vec![]
                }
                Button::Right => {
                    if self.vol_curve_sel + 1 < crate::vol_curve::POINTS {
                        self.vol_curve_sel += 1;
                    }
                    vec![]
                }
                Button::Up if self.vol_curve_ok => self.vol_curve_nudge(1),
                Button::Down if self.vol_curve_ok => self.vol_curve_nudge(-1),
                Button::Back => {
                    self.pop();
                    vec![]
                }
                _ => vec![],
            },
            Screen::Tone => match b {
                Button::Left => {
                    self.tone_sel = self.tone_sel.saturating_sub(1);
                    vec![]
                }
                Button::Right => {
                    if self.tone_sel + 1 < crate::tone::BANDS {
                        self.tone_sel += 1;
                    }
                    vec![]
                }
                Button::Up => self.tone_nudge(1),
                Button::Down => self.tone_nudge(-1),
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
            Screen::VizSet => match b {
                Button::Up => {
                    self.viz_sel = self.viz_sel.saturating_sub(1);
                    vec![]
                }
                Button::Down => {
                    self.viz_sel = (self.viz_sel + 1).min(crate::vizset::ROWS - 1);
                    vec![]
                }
                Button::Select => self.vizset_cycle_row(),
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
            Screen::Alarm => match b {
                Button::Up | Button::Down => {
                    self.alarm_sel ^= 1;
                    vec![]
                }
                Button::Left | Button::Right => {
                    let d = if matches!(b, Button::Left) { -1 } else { 1 };
                    self.alarm_min = crate::alarm::step(self.alarm_min, self.alarm_sel, d);
                    vec![]
                }
                Button::Select => {
                    self.alarm_on = !self.alarm_on;
                    vec![]
                }
                Button::Back => {
                    self.pop();
                    vec![]
                }
                _ => vec![],
            },
            Screen::ClockSet => match b {
                Button::Up => {
                    self.clock_sel = self.clock_sel.saturating_sub(1);
                    vec![]
                }
                Button::Down => {
                    if self.clock_sel + 1 < crate::clockset::FIELDS {
                        self.clock_sel += 1;
                    }
                    vec![]
                }
                Button::Left => {
                    crate::clockset::step(&mut self.clock_fields, self.clock_sel, -1);
                    vec![]
                }
                Button::Right => {
                    crate::clockset::step(&mut self.clock_fields, self.clock_sel, 1);
                    vec![]
                }
                Button::Select => {
                    self.notify(crate::i18n::text("Clock set"));
                    vec![Action::ClockSet]
                }
                Button::Back => {
                    self.pop();
                    vec![]
                }
                _ => vec![],
            },
            Screen::Sound => match b {
                // The cursor walks the rows in the order they are DRAWN (`sound::ORDER`), which
                // since the ENHANCE / SPACE / LEVEL grouping is not the order of their ids.
                Button::Up | Button::Down => {
                    let order = &crate::sound::ORDER;
                    let at = order.iter().position(|r| *r == self.sound_sel).unwrap_or(0);
                    let to = if b == Button::Up {
                        at.saturating_sub(1)
                    } else {
                        (at + 1).min(order.len() - 1)
                    };
                    self.sound_sel = order[to];
                    vec![]
                }
                // Select toggles the focused effect (and recentres the balance); the shell applies
                // it via EffectCtrlDmp. Left/Right nudges the slider when it has focus.
                Button::Select => self.sound_toggle_row(),
                Button::Left => self.sound_lr(-1),
                Button::Right => self.sound_lr(1),
                // Option = instant A/B compare: swap to the other setup.
                Button::Option => self.select_setup(1 - self.setup_idx),
                Button::Back => {
                    self.pop();
                    vec![]
                }
                _ => vec![],
            },
            // The remaining screens (Fm / Receiver / Pairing): Back pops, everything else is a
            // no-op. For Fm and Receiver that is "until their per-screen controls are wired"; for
            // Pairing it is deliberate — every control on that screen (scan, a device row, FORGET,
            // the pairing prompt's yes/no) is a touch target, and this unit has no d-pad, so there
            // is no button that could sensibly stand in for "the third row down".
            //
            // (This comment used to claim there was no Screen::Pairing at all and that pairing.rs
            // was unreachable. Both stopped being true when the route landed; it is corrected here
            // rather than deleted because "designed but unreachable" was worth knowing and is
            // exactly the sort of stale note that sends the next reader looking for a bug.)
            // UpNext isn't here either: it handles buttons above.
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
        crate::now_playing::set_button(self.np_button);
        crate::now_playing::set_back(self.opt(Opt::NpBack));
        // Now Playing grows its own controls with Big view, and onboarding is drawn to fit, so
        // Bigger icons stays out of both (`scale::IconsExempt`).
        let _own_sizes = crate::scale::IconsExempt::new(matches!(
            self.current(),
            Screen::NowPlaying | Screen::Onboarding
        ));
        let _language = crate::i18n::enter(self.language);
        self.reconcile_playing(np.playing);
        // The shell owns the shuffle flag; the MIX chip only needs to know which way it points.
        self.shuffle_on = np.shuffle;
        // NIGHT SPENDS THE BRIGHTNESS ROW ON THE PALETTE, NOT THE BACKLIGHT. In night the shell
        // pins the backlight to raw 1 — the dimmest lit state this panel has — so the row would
        // otherwise do nothing at all in the theme you reach for in the dark. Scaling the palette
        // instead keeps it working, and it is the only remaining way to emit less light: below the
        // backlight floor there is only backlight OFF, which on this transmissive panel is black.
        let theme = {
            let t = Theme {
                style: self.style,
                ..self.palette.theme(self.night, self.accent)
            };
            let t = if self.night {
                let lvl = (self.brightness.clamp(1, 5) - 1) as usize;
                t.scaled(Theme::NIGHT_LEVEL_PCT[lvl])
            } else {
                t
            };
            // Settings ▸ Cover colours: Now Playing only, so the rest of the player keeps its palette.
            let cover = (self.current() == Screen::NowPlaying && self.opt(Opt::NpTint))
                .then(|| np.art_full.or(np.art_thumb))
                .flatten()
                .and_then(|i| i.tint());
            cover.map_or(t, |c| t.tinted(c))
        };
        // The hour on the clock the status bar is about to draw ("14:32"), for SensMe's
        // time-of-day channel. Kept when a frame has no clock rather than forgotten.
        if let Some(h) = np
            .clock
            .split(':')
            .next()
            .and_then(|h| h.trim().parse::<u8>().ok())
            .filter(|h| *h < 24)
        {
            self.clock_hour = Some(h);
        }
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
                    keys_locked: self.opt(Opt::HoldButtons),
                };
                crate::lock::render(c, &theme, fonts, &lk);
            }
            Screen::Zappin => crate::retro_preview::render(
                c,
                &theme,
                fonts,
                np.title,
                np.artist,
                self.zappin_preview_ms,
                self.zappin_active,
                self.zappin_completed,
                np.playing,
                self.zappin_source,
                self.zappin_start_ms,
                self.context_idx() < self.context().len(),
                self.zappin_elapsed_ms,
                self.zappin_length_ms,
                self.zappin_current,
                self.zappin_total,
            ),
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
                    lyrics: self.lyrics_button(),
                    ..*np
                };
                if let (true, Some(tape)) = (self.tape_view(), np2.tape) {
                    let row = self.context.get(self.context_idx);
                    let year = row.map_or(0, |r| r.year);
                    let track = row.map_or(0, |r| r.track);
                    let (year, track) = (
                        if year > 0 {
                            year.to_string()
                        } else {
                            String::new()
                        },
                        if track > 0 {
                            track.to_string()
                        } else {
                            String::new()
                        },
                    );
                    let song = crate::tape::Song {
                        title: np2.title,
                        artist: np2.artist,
                        album: row.map_or("", |r| r.art.as_str()),
                        year: &year,
                        track: &track,
                        duration: row.map_or(0, |r| crate::tape::secs(&r.dur)),
                    };
                    // The reel's clock: the visualiser phase, which runs only while it plays.
                    let ms = (np2.viz_seed / 0.0036) as u32;
                    crate::tape::render(c, &theme, fonts, &np2, tape, &song, ms);
                } else if let (true, Some(skin)) = (self.winamp_view(), np2.skin) {
                    let view = crate::winamp::View {
                        queue: &self.context,
                        at: self.context_idx,
                        first: self.wa_pl_first(),
                        volume: if self.bt_route {
                            (self.bt_volume, crate::overlay::BT_VOL_MAX)
                        } else {
                            (self.volume, crate::overlay::VOL_MAX)
                        },
                        balance: self.snd_balance,
                        remaining: self.wa_remaining,
                        playlist: self.wa_playlist,
                        vis: self.wa_vis,
                        style: self.wa_style,
                        title_shift: self.wa_title_shift,
                    };
                    crate::winamp::render(c, &theme, fonts, &np2, skin, &view);
                } else {
                    crate::now_playing::render(c, &theme, fonts, &np2);
                    // sleep-timer countdown badge (nav owns the live remaining minutes)
                    crate::now_playing::sleep_badge(
                        c,
                        &theme,
                        fonts,
                        self.sleep_min,
                        self.stop_after,
                    );
                    // Settings ▸ Back arrow and Track number (#51), in the top band.
                    if self.opt(Opt::NpBack) {
                        crate::now_playing::back_chip(c, &theme);
                    }
                    if self.opt(Opt::NpCount) {
                        crate::now_playing::count_chip(
                            c,
                            &theme,
                            fonts,
                            self.context_idx + 1,
                            self.context.len(),
                        );
                    }
                    // The queue toolbar slot becomes a persistent, touch-sized return to the preview
                    // while it is active. Its existing glyph remains; this short caption is the state
                    // marker and the whole 120px slot is already the hit target.
                    if self.zappin_active {
                        let marker = crate::widgets::sty(
                            crate::text::Family::Mono,
                            crate::text::Weight::Bold,
                            9.0,
                            theme.acc,
                            0.04,
                        );
                        crate::widgets::center(
                            c,
                            fonts,
                            180.0,
                            739.0,
                            crate::i18n::text("PREVIEW"),
                            &marker,
                        );
                    }
                }
            }
            Screen::Menu => {
                let subs = self.menu_subtitles();
                let preview_sub = self.zappin_menu_subtitle();
                // The strip answers "what's on?" without a trip to the player.
                let strip = if np.title.is_empty() {
                    String::from(crate::i18n::text("NOTHING PLAYING"))
                } else {
                    let mut line = crate::ui_format!("NOW › {}", np.title);
                    // No elapsed time: the strip is one fitted line, and the tail is what gets cut —
                    // a clock that reads "1:…" says less than no clock.
                    if !np.artist.is_empty() {
                        line.push_str(" · ");
                        line.push_str(np.artist);
                    }
                    line
                };
                let home_row = match self.home_screen {
                    HOME_LIBRARY => Some(Screen::Library),
                    _ => None,
                };
                let items: Vec<MenuItem> = self
                    .menu_visible()
                    .into_iter()
                    .enumerate()
                    .map(|(i, (screen, label, value))| MenuItem {
                        label,
                        sub: match *screen {
                            Screen::Library => &subs.library,
                            Screen::Folders => &subs.folders,
                            Screen::SensMe => &subs.sensme,
                            Screen::Sound => &subs.sound,
                            Screen::Bluetooth => &subs.bluetooth,
                            Screen::UsbDac => &subs.usb_dac,
                            Screen::Soundscape => &subs.soundscape,
                            Screen::Wifi => &subs.wifi,
                            Screen::Zappin => &preview_sub,
                            _ => crate::i18n::text(value),
                        },
                        home: home_row == Some(*screen),
                        active: i == self.menu_idx,
                        icon: self.opt(Opt::MenuIcons).then(|| menu_icon(*screen)),
                    })
                    .collect();
                crate::menu::render(
                    c,
                    &theme,
                    fonts,
                    &strip,
                    &items,
                    self.home_screen as usize,
                    self.menu_scroll(),
                );
            }
            Screen::Library => {
                // Record the tab strip exactly as drawn, so `tap` hits the labels the user sees.
                *self.lib_tab_zones.borrow_mut() =
                    crate::library::tab_layout(fonts, self.lib.folders_tab);
                crate::library::render(
                    c,
                    &theme,
                    fonts,
                    self.lib_tab,
                    self.lib_idx,
                    self.lib_scroll_px,
                    self.lib_sort,
                    self.album_sort,
                    self.album_expanded,
                    &self.lib,
                    self.swipe_row,
                    self.sbar_active(),
                    self.lib_band(),
                    self.search_enabled,
                );
                let az = self.az_present_memo();
                crate::library::az_render(
                    c,
                    &theme,
                    fonts,
                    self.lib_tab,
                    &az,
                    self.lib_sort,
                    self.album_sort,
                );
                if let Some(l) = self
                    .sbar_az
                    .as_ref()
                    .and_then(|o| library::az_letter_at_scroll(o, self.lib_scroll_px))
                {
                    crate::library::az_bubble(c, &theme, fonts, self.lib_tab, l);
                }
            }
            Screen::Album => {
                let flat = self.lib.albums_flat();
                if let Some(al) = flat.get(self.album_view) {
                    let playing = self.album_playing_row(al);
                    crate::library::album_view(
                        c,
                        &theme,
                        fonts,
                        al,
                        playing,
                        self.album_scroll_px,
                        self.album_cover.as_ref(),
                        self.swipe_row,
                        self.sbar_active(),
                        self.lib.album_rating(al),
                        self.opt(Opt::DeleteMusic),
                    );
                } else {
                    crate::library::render(
                        c,
                        &theme,
                        fonts,
                        self.lib_tab,
                        self.lib_idx,
                        self.lib_scroll_px,
                        self.lib_sort,
                        self.album_sort,
                        self.album_expanded,
                        &self.lib,
                        self.swipe_row,
                        self.sbar_active(),
                        self.lib_band(),
                        self.search_enabled,
                    );
                }
            }
            Screen::Playlist => match self.playlist_row() {
                Some(pl) => crate::library::playlist_view(
                    c,
                    &theme,
                    fonts,
                    &self.lib,
                    pl,
                    self.playlist_scroll_px,
                    self.playing_object_id()
                        .and_then(|id| pl.track_list.iter().position(|s| s.object_id == id)),
                    self.swipe_row,
                    self.sbar_active(),
                ),
                // The library reloaded and the index is stale — fall back to the tab rather than
                // drawing a blank page the user cannot leave.
                None => crate::library::render(
                    c,
                    &theme,
                    fonts,
                    self.lib_tab,
                    self.lib_idx,
                    self.lib_scroll_px,
                    self.lib_sort,
                    self.album_sort,
                    self.album_expanded,
                    &self.lib,
                    self.swipe_row,
                    self.sbar_active(),
                    self.lib_band(),
                    self.search_enabled,
                ),
            },
            Screen::Artist => match self.artist_page() {
                Some(page) => crate::library::artist_view(
                    c,
                    &theme,
                    fonts,
                    &self.lib,
                    &page,
                    self.artist_scroll_px,
                    self.artist_track_idx,
                    self.swipe_row,
                    self.sbar_active(),
                ),
                // The artist index outlived its library (a rescan while the page was open).
                // Falling back to the list is better than a blank screen, and Back still works.
                None => crate::library::render(
                    c,
                    &theme,
                    fonts,
                    self.lib_tab,
                    self.lib_idx,
                    self.lib_scroll_px,
                    self.lib_sort,
                    self.album_sort,
                    self.album_expanded,
                    &self.lib,
                    self.swipe_row,
                    self.sbar_active(),
                    self.lib_band(),
                    self.search_enabled,
                ),
            },
            Screen::Onboarding => crate::onboarding::render_with(
                c,
                &theme,
                fonts,
                self.onboarding_page,
                &crate::onboarding::Choices {
                    big: crate::scale::large(),
                    bar: crate::scale::big_bar_set(),
                    icons: crate::scale::big_icons_set(),
                    quick: self.opt(Opt::Quick),
                    folders: self.opt(Opt::FoldersTab),
                },
            ),
            Screen::Help => {
                crate::help::render(c, &theme, fonts, self.help_scroll_px, self.opt(Opt::Quick))
            }
            Screen::UsbStorage => crate::usb_storage::render(c, &theme, fonts),
            Screen::GenreFilter => crate::library::genre_render(
                c,
                &theme,
                fonts,
                &self.lib,
                self.genre_scroll_px,
                self.sbar_active(),
            ),
            Screen::Folders => crate::folders::render(
                c,
                &theme,
                fonts,
                &self.lib,
                self.folder_cur(),
                self.folder_scroll_px,
                self.sbar_active(),
                &self.folder_sel,
            ),
            Screen::Books => crate::audiobooks::render(
                c,
                &theme,
                fonts,
                &self.lib,
                self.books_scroll_px,
                self.sbar_active(),
            ),
            Screen::SensMe => crate::sensme::render(
                c,
                &theme,
                fonts,
                &self.lib,
                self.sensme_channel,
                self.sensme_scroll_px,
                self.sbar_active(),
                crate::sensme::Foot {
                    follow: self.sensme_follow,
                    hour: self.clock_hour,
                },
            ),
            Screen::TrackInfo => {
                self.track_info_h = crate::track_info::content_h(fonts, &theme, &self.track_info);
                self.track_info_rows =
                    crate::track_info::row_heights(fonts, &theme, &self.track_info);
                let max = self.track_info_max_scroll();
                self.track_info_scroll_px = self.track_info_scroll_px.clamp(0, max);
                let rating = self
                    .playing_object_id()
                    .map_or(0, |id| self.lib.stat(id).rating);
                crate::track_info::render(
                    c,
                    &theme,
                    fonts,
                    &self.track_info,
                    self.track_info_scroll_px,
                    self.sbar_active(),
                    rating,
                    (!self.presets.is_empty()).then(|| self.album_sound_row().unwrap_or((0, 0))),
                    self.opt(Opt::DeleteMusic),
                )
            }
            Screen::Lyrics => {
                let cur = self
                    .lyrics
                    .as_ref()
                    .and_then(|l| l.current(self.lyrics_pos_ms));
                self.lyrics_heights = self
                    .lyrics
                    .as_ref()
                    .map(|l| crate::lyrics::heights(fonts, &theme, l))
                    .unwrap_or_default();
                // Following writes the scroll itself, so a drag that ends following starts from
                // exactly where the page was rather than jumping back to the top.
                if let (true, Some(i)) = (self.lyrics_follow, cur) {
                    self.lyrics_scroll_px = crate::lyrics::follow_scroll(&self.lyrics_heights, i);
                }
                let max = self.lyrics_max_scroll();
                self.lyrics_scroll_px = self.lyrics_scroll_px.clamp(0, max);
                crate::lyrics::render(
                    c,
                    &theme,
                    fonts,
                    self.lyrics.as_ref(),
                    cur,
                    self.lyrics_scroll_px,
                    self.sbar_active(),
                )
            }
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
                // DO EVERY MUTATION FIRST, THEN BORROW. This used to clone the whole context —
                // `self.context.clone()` — with the note "cloned rather than borrowed because the
                // auto-follow below has to WRITE queue_scroll_px". That was a borrow-checker
                // workaround costing 499 us a frame on the host (~7 ms on device) and about
                // 15,000 string allocations, because after a "Shuffle all songs" the context is
                // the entire library. It was the largest allocation churn left in the app, on a
                // heap whose churn has already caused one on-device allocator abort.
                //
                // Nothing about it was necessary: the follow needs only three numbers out of the
                // context (its length, the current index, the queue length). Compute the scroll,
                // finish writing, and only then take the shared borrow the view wants.
                let cur = (!self.context.is_empty()).then_some(self.context_idx);
                let cur_id = self.playing_object_id();
                // AUTO-FOLLOW. `render` is the only place that knows what is playing, so the snap
                // lives here: whenever the current track moves (or we have just arrived on the
                // screen) and the user has not taken the list over by scrolling it, park NOW
                // PLAYING a third of the way down. The id is watched as well as the index because
                // a shuffle can change the track at an index without moving the index.
                let track_changed = self.up_next_cur != cur || self.up_next_id != cur_id;
                self.up_next_cur = cur;
                self.up_next_id = cur_id;
                if self.queue_follow && track_changed {
                    // O(1) arithmetic rather than `up_next_layout()`, which materialises one slot
                    // per track to be asked for a single row's top.
                    self.queue_scroll_px =
                        crate::up_next::metrics(self.history.len(), self.context.len(), cur)
                            .follow_scroll();
                    self.fling_v = 0.0;
                }
                // The NEXT heading names the playing album while everything still to come is from
                // it, and says NEXT UP once anything else is in the list. Stops at the first row
                // from elsewhere, so a shuffled library costs one comparison, not 3,600.
                let album = match self.context.get(self.context_idx) {
                    Some(t)
                        if self.context[self.context_idx + 1..]
                            .iter()
                            .all(|r| r.art == t.art) =>
                    {
                        t.art.clone()
                    }
                    _ => String::new(),
                };
                // ponytail: keyed on the list's address, length and position; an edit that
                // swaps a row in place for one of another length shows the old total until the
                // track changes. Key on a queue generation if that ever matters.
                let key = (
                    self.context.as_ptr() as usize,
                    self.context.len(),
                    self.context_idx,
                );
                let after = match self.queue_after_memo {
                    Some((k, v)) if k == key => v,
                    _ => {
                        let v = crate::up_next::after_secs(&self.context, cur);
                        self.queue_after_memo = Some((key, v));
                        v
                    }
                };
                let view = crate::up_next::QueueView {
                    album: &album,
                    tracks: &self.context,
                    current: cur,
                    history: &self.history,
                    lib: &self.lib,
                    scroll_px: self.queue_scroll_px,
                    drag: self.row_drag,
                    swipe: self.swipe_row,
                    sbar_active: self.sbar_active(),
                    left_secs: crate::up_next::left_secs(after, np.remaining),
                };
                crate::up_next::render_view(c, &theme, fonts, &view);
            }
            Screen::Eq => crate::eq::render(
                c,
                &theme,
                fonts,
                &self.eq_bands,
                data::EQ_PRESETS[self.eq_preset].0,
                self.eq_sel,
                self.eq_off_reason(),
            ),
            Screen::Sound => {
                let snd = Sound {
                    nc: self.nc_items().map(|(item, _)| {
                        crate::nc::MODES
                            .iter()
                            .find(|m| m.2 == item)
                            .map_or("Off", |m| m.0)
                    }),
                    clearaudio_ok: self.clearaudio_ok,
                    dc_gone: self.clear_bass_available,
                    vpt_gone: self.w1_chain,
                    dsee: self.snd_dsee,
                    vinyl: self.snd_vinyl,
                    vpt: self.vpt_label(),
                    vpt_on: self.snd_vpt,
                    dcphase: self.dc_label(),
                    dcphase_on: self.snd_dc,
                    normalizer: self.snd_norm,
                    clearaudio: self.snd_clear,
                    balance: self.snd_balance,
                    balance_drag: matches!(self.scrub, Scrub::Balance),
                    bt_route: self.bt_route,
                    mono: self.mono,
                    // IS MONO ACTUALLY REACHING WHAT IS PLAYING? Without the mono library, only on
                    // the one path Cinder owns the PCM for — USB-DAC in, LDAC out, where the bridge
                    // does the downmix itself: ordinary playback goes through Sony's SoundServiceFw
                    // to a codec with no channel-sum control, and Bluetooth transmit never passes
                    // Cinder either. With libcinder_mono.so loaded in SoundServiceFw it reaches
                    // both. `analysis/RE_mono_audio.md` is the evidence; the row says which.
                    mono_live: self.usb_dac_on || self.mono_shim,
                    // Both are set on OTHER screens and both change what this one's footer means:
                    // Source Direct (Sound ▸ Advanced) bypasses the whole chain, and Tone Control
                    // replaces the 10-band EQ rather than stacking with it.
                    source_direct: self.adv_source_direct,
                    tone_control: self.adv_tone,
                    presets_saved: self.presets.len(),
                    hp_eq: self.mono_shim && !self.hp_eq.is_empty(),
                    crossfeed: self.mono_shim && self.opt(Opt::Crossfeed),
                    eq_on_dac: self.eq_on_dac(),
                    eq_preset: data::EQ_PRESETS[self.eq_preset].0,
                    // The signal path shows what is REALLY carrying the audio: it ends at
                    // Bluetooth only while a sink is the route. (It used to end there whenever the
                    // radio was merely ON, so with nothing connected the strip said BT·LDAC over
                    // audio leaving by the jack — and since R5 the Profile row beneath it says
                    // "3.5 mm now", two answers on one screen.) Prefer the codec the link
                    // negotiated; until the shell has read it, the preference stands in.
                    bt_codec: if self.bt_route {
                        self.negotiated_codec_name().or(Some(self.bt_codec_name()))
                    } else {
                        None
                    },
                };
                crate::sound::render(
                    c,
                    &theme,
                    fonts,
                    &snd,
                    self.sound_sel,
                    self.setup_idx,
                    self.sound_scroll(),
                )
            }
            Screen::Nc => crate::nc::render(
                c,
                &theme,
                fonts,
                &crate::nc::Nc {
                    mode: self.nc_mode,
                    model: self.nc_model,
                    ambient_ok: self.nc_ambient_ok,
                },
            ),
            Screen::Presets => {
                let saved: Vec<String> = self.presets.iter().map(describe_setup).collect();
                let p = crate::presets::Presets {
                    saved: &saved,
                    names: &self.preset_names,
                    scroll: self.presets_scroll_px,
                    armed: self.preset_armed,
                    live: crate::profile::letter(self.setup_idx),
                    follow: self.profile_follow,
                };
                crate::presets::render(c, &theme, fonts, &p)
            }
            Screen::DacEq => {
                let back = match self.stack.iter().rev().nth(1) {
                    Some(Screen::Sound) => crate::i18n::text("Sound"),
                    _ => crate::i18n::text("Advanced"),
                };
                let d = crate::dac_eq::DacEq {
                    bands: self.dac_eq,
                    direct: self.adv_source_direct,
                    jack: self.jack_eq_dac,
                    back,
                };
                crate::dac_eq::render(c, &theme, fonts, &d, self.dac_eq_sel)
            }
            Screen::VolCurve => {
                let v = crate::vol_curve::VolCurve {
                    pts: self.vol_curve,
                    on: self.vol_curve_on,
                    available: self.vol_curve_ok,
                };
                crate::vol_curve::render(c, &theme, fonts, &v, self.vol_curve_sel)
            }
            Screen::Soundscape => {
                let v = self.soundscape_view();
                crate::soundscape::render(c, &theme, fonts, self.ambient_sel, &v)
            }
            Screen::Tone => {
                let tc = crate::tone::Tone {
                    bands: self.tone_bands,
                    on: self.adv_tone,
                    overridden_by: if self.adv_source_direct {
                        Some(crate::i18n::text("Direct mode"))
                    } else if self.snd_clear {
                        Some("ClearAudio+")
                    } else {
                        None
                    },
                };
                crate::tone::render(c, &theme, fonts, &tc, self.tone_sel)
            }
            Screen::Device => {
                crate::device::render(c, &theme, fonts, &self.device_view(), self.device_scroll_px)
            }
            Screen::Storage => crate::storage::render(
                c,
                &theme,
                fonts,
                &self.storage_view(),
                self.storage_scroll_px,
            ),
            Screen::Advanced => {
                let adv = crate::advanced::Advanced {
                    source_direct: self.adv_source_direct,
                    clear_phase: self.adv_clear_phase,
                    dsee_ai: self.adv_dsee_ai,
                    // The parent switch lives on the Sound screen; both DSEE rows below are only
                    // in the path while it is on.
                    dsee_hx: self.snd_dsee,
                    dsee_custom: if self.adv_dsee_custom {
                        crate::advanced::DSEE_MODES[self
                            .adv_dsee_mode
                            .min(crate::advanced::DSEE_MODES.len() - 1)]
                    } else {
                        "Off"
                    },
                    dsee_custom_on: self.adv_dsee_custom,
                    vinyl_type: crate::advanced::VINYL_TYPES[self
                        .adv_vinyl_type
                        .min(crate::advanced::VINYL_TYPES.len() - 1)],
                    vinyl_on: self.snd_vinyl,
                    vpt: self.vpt_label(),
                    vpt_on: self.snd_vpt,
                    dcphase: self.dc_label(),
                    dcphase_on: self.snd_dc,
                    tone_control: self.adv_tone,
                    hp_linear: self.adv_hp_linear,
                    crossfeed: self.opt(Opt::Crossfeed),
                    mono: self.mono,
                    replaygain: crate::advanced::REPLAYGAIN[self.replaygain_mode()],
                    hp_eq: &self.hp_eq,
                    hook: self.mono_shim,
                    dac_eq_on: self.dac_eq != [0; crate::dac_eq::BANDS],
                    vol_curve: self.vol_curve_ok.then_some(self.vol_curve_on),
                    clear_bass: self
                        .clear_bass_available
                        .then(|| crate::advanced::CLEAR_BASS[self.adv_clear_bass]),
                    clear_bass_on: self.adv_clear_bass != 0,
                    // Which upstream control is hiding the rest, if any. Source Direct wins the
                    // naming when both are on: it is the outer bypass, so it is what you would
                    // have to turn off FIRST.
                    overridden_by: if self.adv_source_direct {
                        Some(crate::i18n::text("Direct mode"))
                    } else if self.snd_clear {
                        Some("ClearAudio+")
                    } else {
                        None
                    },
                    w1_chain: self.w1_chain,
                };
                crate::advanced::render(c, &theme, fonts, &adv, self.adv_sel, self.adv_scroll_px)
            }
            Screen::VizSet => {
                let (rate, window_ms) = self.viz_analyzer_params();
                let rate_lbl = format!("{rate} HZ");
                let bands_lbl = crate::vizcfg::bands_from_index(self.viz_bands).to_string();
                let range_lbl = format!(
                    "{} DB",
                    crate::vizcfg::range_from_index(self.viz_range) as i32
                );
                let vs = crate::vizset::VizSet {
                    style: crate::viz::name_upper(self.viz_kind),
                    cover: crate::viz::size_name(self.viz_size),
                    scale: crate::vizcfg::Scale::from_index(self.viz_scale).name(),
                    range: &range_lbl,
                    response: crate::vizcfg::response_from_index(self.viz_response).2,
                    curve: crate::vizcfg::Interp::from_index(self.viz_interp).name(),
                    peaks: self.viz_peak_hold,
                    window: crate::vizcfg::window_name(self.viz_window),
                    rate: &rate_lbl,
                    bands: &bands_lbl,
                    columns: crate::vizcfg::bands_from_index(self.viz_bands),
                    // The preview shows what Now Playing would show: the same levels, the same
                    // markers, the same style. `viz_levels` is None whenever the analyzer is not
                    // streaming, and the screen says so rather than passing synthetic motion off
                    // as the signal.
                    levels: np.viz_levels,
                    peak_marks: np.viz_peaks,
                    sig: np.viz_sig,
                    seed: np.viz_seed,
                    kind: crate::viz::from_index(self.viz_kind),
                };
                let _ = window_ms;
                crate::vizset::render(c, &theme, fonts, &vs, self.viz_sel)
            }
            Screen::Bluetooth => {
                // Not drawn on this screen; the row lives on the codec page.
                let bt = self.bt_view(crate::i18n::text("OFF"));
                crate::bluetooth::render(c, &theme, fonts, &bt)
            }
            Screen::BtCodec => {
                let fine_lbl: &str = match self.bt_fine_span() {
                    0 => crate::i18n::text("OFF"),
                    1 => "±1 dB",
                    2 => "±2 dB",
                    _ => "±3 dB",
                };
                let bt = self.bt_view(fine_lbl);
                crate::bluetooth::render_codec(c, &theme, fonts, &bt, self.bt_codec_scroll())
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
                    self.busy_phase,
                    self.bt_paired_page,
                );
                // Drawn last so it sits over the list, matching how the tap handler treats it.
                if let Some(p) = &self.bt_prompt {
                    crate::pairing::render_prompt(c, &theme, fonts, p);
                }
            }
            Screen::Settings => {
                let sleep_lbl = self.sleep_label();
                let alarm_lbl = if self.alarm_on {
                    crate::alarm::hhmm(self.alarm_min)
                } else {
                    crate::i18n::text("OFF").to_string()
                };
                let brightness_lbl = if self.brightness == 0 {
                    crate::i18n::text("BACKLIGHT OFF").to_string()
                } else {
                    format!("{} / 5", self.brightness)
                };
                let screen_off_lbl = screen_off_label(self.screen_off_s);
                let auto_off_lbl = auto_off_label(self.auto_off_min);
                // The row value doubles as the confirmation prompt — no extra screen needed, and
                // the armed state is impossible to miss because it replaces the value in place.
                let boot_stock_lbl = if self.boot_stock_armed {
                    crate::i18n::text("TAP AGAIN")
                } else {
                    "SONY"
                };
                let clock_lbl = self.clock_label(np.clock);
                let db_label = self.database_label();
                let view = crate::settings::SettingsView {
                    volume_limit: self.volume_limit,
                    big_view: crate::scale::large(),
                    big_bar: crate::scale::big_bar_set(),
                    big_icons: crate::scale::big_icons_set(),
                    ignore_the: self.ignore_the,
                    opts: self.opts,
                    swipe_up: self.np_swipe_up,
                    swipe_down: self.np_swipe_down,
                    np_button: self.np_button,
                    shuffle_by: self.shuffle_by.label(),
                    usb_dac: self.usb_dac_on,
                    battery_care: self.battery_care,
                    device: &self.device_summary(),
                    database: &db_label,
                    storage: self.storage_label(),
                    sleep: &sleep_lbl,
                    alarm: &alarm_lbl,
                    brightness: &brightness_lbl,
                    screen_off: &screen_off_lbl,
                    auto_off: &auto_off_lbl,
                    bt_idle_off: self.bt_idle_off,
                    boot_stock: boot_stock_lbl,
                    clock: &clock_lbl,
                    page: self.settings_page,
                    languages: &self.languages,
                };
                crate::settings::render(
                    c,
                    &theme,
                    fonts,
                    self.settings_sel,
                    self.settings_scroll_px,
                    &view,
                )
            }
            Screen::Display => {
                // "BARS · VEIL" — style and cover size, the two facts the Visualiser page owns.
                let viz_lbl = format!(
                    "{} · {}",
                    crate::viz::name_upper(self.viz_kind),
                    crate::viz::size_name(self.viz_size)
                );
                let view = crate::display::DisplayView {
                    palette: self.palette_name(),
                    accent_locked: self.palette_pins_accent(),
                    accent: self.accent,
                    night: self.night,
                    night_sched: self.night_sched,
                    night_from: self.night_from,
                    night_until: self.night_until,
                    volume_hud: self.volume_hud,
                    viz: &viz_lbl,
                    style: self.style,
                };
                crate::display::render(
                    c,
                    &theme,
                    fonts,
                    self.display_sel,
                    &view,
                    self.display_scroll(),
                )
            }
            Screen::Skins => crate::skins::render(
                c,
                &theme,
                fonts,
                self.style == crate::style::Style::Cassette,
                &self.skin_names,
                &self.skin_cur,
                self.skins_scroll_px,
            ),
            Screen::Language => crate::language::render(
                c,
                &theme,
                fonts,
                &self.languages,
                self.language,
                self.language_scroll_px,
            ),
            Screen::Palette => {
                let entries: Vec<crate::palette_list::Entry> = self
                    .palette_order()
                    .into_iter()
                    .map(|slot| {
                        let (name, sub, tokens) = match slot {
                            None => (
                                crate::palette::BUILTIN_NAME.to_string(),
                                crate::i18n::text("Built in").to_string(),
                                crate::theme::CINDER,
                            ),
                            Some(i) => {
                                let p = &self.palettes[i];
                                (p.name.clone(), format!("{}.palette", p.id), p.tokens)
                            }
                        };
                        let th = tokens.theme(self.night, self.accent);
                        crate::palette_list::Entry {
                            name,
                            sub,
                            cells: [th.bg, th.line, th.dim, th.ink, th.acc],
                            active: slot == self.palette_active,
                        }
                    })
                    .collect();
                let skipped: Vec<crate::palette_list::Skipped> = self
                    .palette_skipped
                    .iter()
                    .map(|m| crate::palette_list::skipped_from(m))
                    .collect();
                crate::palette_list::render(
                    c,
                    &theme,
                    fonts,
                    self.palette_sort,
                    &entries,
                    &skipped,
                    self.palette_scroll_px,
                )
            }
            Screen::ClockSet => {
                crate::clockset::render(c, &theme, fonts, &self.clock_fields, self.clock_sel)
            }
            Screen::Alarm => crate::alarm::render(
                c,
                &theme,
                fonts,
                self.alarm_min,
                self.alarm_on,
                self.alarm_sel,
            ),
            Screen::Wifi => crate::wifi::render(
                c,
                &theme,
                fonts,
                &crate::wifi::View {
                    status: &self.wifi_status,
                    radio_note: &self.radio_row_note(),
                    speakers_note: &self
                        .cast_speaker
                        .as_ref()
                        .map(|s| crate::ui_format!("Playing on {}", s))
                        .unwrap_or_default(),
                    remote_note: &self.remote_row_note(),
                    airplay: self.opt(Opt::WifiAirplay),
                    airplay_note: &self.airplay_note,
                    update: if self.update_offered() {
                        &self.update_offer
                    } else {
                        ""
                    },
                    update_note: &self.update_install_note,
                },
            ),
            Screen::WifiOptions => crate::wifi::render_options(
                c,
                &theme,
                fonts,
                &crate::wifi::OptionsView {
                    options: [
                        self.opt(Opt::WifiAutostart),
                        self.opt(Opt::WifiClock),
                        self.opt(Opt::WifiLb),
                        self.opt(Opt::WifiUpdates),
                        self.opt(Opt::WifiLyrics),
                        self.opt(Opt::WifiArt),
                        self.opt(Opt::WifiPlayTo),
                        self.opt(Opt::WifiDrive),
                    ],
                    lb_note: &self.wifi_lb_note,
                    update_note: &self.wifi_update_note,
                    play_to_note: &self.play_to_note,
                    drive_note: &self.drive_note,
                },
            ),
            Screen::NetRadio => crate::netradio::render(
                c,
                &theme,
                fonts,
                &self.radio_stations,
                &self.radio_status,
                self.radio_scroll_px,
            ),
            Screen::RadioFind => crate::netradio::render_found(
                c,
                &theme,
                fonts,
                &crate::netradio::FoundView {
                    query: &self.radio_query,
                    found: &self.radio_found,
                    searching: self.radio_finding,
                    note: &self.radio_find_note,
                    have: &self.radio_stations,
                },
            ),
            Screen::WifiSpeakers => crate::wifi::render_speakers(
                c,
                &theme,
                fonts,
                &crate::wifi::SpeakersView {
                    speakers: &self.speakers,
                    scanning: self.speakers_scanning,
                    connected: matches!(self.wifi_status.state, crate::wifi::State::Connected),
                    casting: self.cast_speaker.as_deref(),
                    line: &self.cast_line,
                },
            ),
            Screen::MediaServers => crate::media_server::render(
                c,
                &theme,
                fonts,
                &crate::media_server::View {
                    level: self.media_levels.last(),
                    busy: self.media_busy,
                    note: &self.media_note,
                    playing: &self.media_playing,
                },
            ),
            Screen::WifiRemote => crate::wifi::render_remote(
                c,
                &theme,
                fonts,
                &crate::wifi::RemoteView {
                    on: self.opt(Opt::WifiRemote),
                    serving: self.remote_on,
                    phones: self.remote_phones,
                    wifi_connected: matches!(self.wifi_status.state, crate::wifi::State::Connected),
                    address: &self.remote_address(),
                    name_address: &crate::ui_format!("or cinder.local:{}", self.remote_port),
                    code: &self.remote_code,
                },
            ),
            Screen::WifiNetworks => crate::wifi::render_list(
                c,
                &theme,
                fonts,
                &crate::wifi::ListView {
                    status: &self.wifi_status,
                    networks: &self.wifi_networks,
                    scanning: self.wifi_scanning,
                    wps: self.wifi_wps,
                    sel: None,
                },
            ),
            Screen::Fm => {
                let v = crate::fm::Fm {
                    khz: self.fm_khz,
                    playing: self.fm_playing,
                    stations: self.fm_stations,
                    n_stations: self.fm_n_stations,
                    scanning: self.fm_scanning,
                    scan_pct: self.fm_scan_pct,
                    antenna: self.fm_antenna,
                    bt_out: self.fm_bt_out,
                    signal: self.fm_signal,
                    hw: self.fm_hw,
                    stereo: self.fm_stereo,
                    rec_available: self.fm_rec_available,
                    rec_secs: self.fm_rec_secs,
                };
                crate::fm::render(c, &theme, fonts, &v)
            }
            Screen::UsbDac => {
                let ldac = self.usb_dac_on && self.bt_on;
                let codec = self.bt_codec_name();
                let dev: Option<&str> = None; // see the Bluetooth screen: no invented device name
                crate::usbdac::render(
                    c,
                    &theme,
                    fonts,
                    self.usb_dac_on,
                    ldac,
                    codec,
                    dev,
                    data::EQ_PRESETS[self.eq_preset].0,
                    self.snd_dsee,
                    self.usb_dac_fmt,
                    self.negotiated_codec_name(),
                    self.usb_dac_ll_avail.then_some(self.usb_dac_ll),
                    self.usb_dac_ll_stats.as_ref(),
                )
            }
            Screen::Receiver => {
                let rx = crate::receiver::Rx {
                    on: self.rx_on,
                    phase: if self.rx_on { self.rx_phase.max(1) } else { 0 },
                    peer: &self.rx_peer,
                    codec: self.rx_codec,
                    freq: self.rx_freq,
                    bitrate: self.rx_bitrate,
                    radio: self.bt_on,
                    codec_pref: self.rx_codec_pref,
                };
                crate::receiver::render(c, &theme, fonts, &rx);
                if let Some(p) = &self.bt_prompt {
                    crate::pairing::render_prompt(c, &theme, fonts, p);
                }
            }
            Screen::Keyboard => {
                // THE KEYBOARD HAS TO SAY WHAT IT IS FOR. This match had two arms and a catch-all,
                // so the moment a third purpose existed the search opened a screen headed
                // "New playlist" with the placeholder "Playlist name" — and the only reasonable
                // reading of that is that search does not work. Reported 2026-09-10: "the playlist
                // search to add doesn't actually search the songs". The filter was fine; the screen
                // was lying about its own job.
                let (title, placeholder) = match self.kb_purpose {
                    KbPurpose::Rename(_) => (
                        crate::i18n::text("Rename playlist").to_string(),
                        crate::i18n::text("Playlist name"),
                    ),
                    KbPurpose::TrackSearch => (
                        // The live count is the proof that it IS searching, while you type.
                        if self.kb_text.trim().is_empty() {
                            crate::i18n::text("Find a song").to_string()
                        } else {
                            crate::ui_format!("{} matching", self.track_pick_order.len())
                        },
                        crate::i18n::text("Title, artist or album"),
                    ),
                    KbPurpose::LibrarySearch => (
                        if self.kb_text.trim().is_empty() {
                            crate::i18n::text("Search the library").to_string()
                        } else {
                            crate::ui_format!("{} matching", self.search_order.len())
                        },
                        crate::i18n::text("Title, artist or album"),
                    ),
                    KbPurpose::SaveQueue => (
                        crate::i18n::text("Save Up Next").to_string(),
                        crate::i18n::text("Playlist name"),
                    ),
                    KbPurpose::ViewName => (
                        crate::i18n::text("Name it").to_string(),
                        crate::i18n::text("Smart playlist name"),
                    ),
                    KbPurpose::PresetName(None) => (
                        crate::i18n::text("Save current sound").to_string(),
                        crate::i18n::text("Preset name"),
                    ),
                    KbPurpose::WifiPassword => (
                        crate::i18n::text("Wi-Fi password").to_string(),
                        crate::i18n::text("Password"),
                    ),
                    KbPurpose::RadioFind => (
                        crate::i18n::text("Find a station").to_string(),
                        crate::i18n::text("Name or genre"),
                    ),
                    KbPurpose::MediaSearch => (
                        crate::i18n::text("Search this server").to_string(),
                        crate::i18n::text("Title, artist or album"),
                    ),
                    KbPurpose::PresetName(Some(_)) => (
                        crate::i18n::text("Rename preset").to_string(),
                        crate::i18n::text("Preset name"),
                    ),
                    _ => (
                        crate::i18n::text("New playlist").to_string(),
                        crate::i18n::text("Playlist name"),
                    ),
                };
                crate::keyboard::render(
                    c,
                    &theme,
                    fonts,
                    &title,
                    &self.kb_text,
                    placeholder,
                    self.kb_page,
                    self.kb_shift,
                    matches!(
                        self.kb_purpose,
                        KbPurpose::TrackSearch | KbPurpose::LibrarySearch
                    ),
                )
            }
            Screen::PlaylistEdit => match self.pl_edit.as_ref() {
                Some(e) => crate::playlist_edit::render(
                    c,
                    &theme,
                    fonts,
                    &crate::playlist_edit::EditView {
                        name: &e.name,
                        cover: &self.cover_label(),
                        rows: e.order.iter().filter_map(|&i| e.rows.get(i)).collect(),
                        glued: e
                            .order
                            .iter()
                            .map(|&i| e.glued.get(i).copied().unwrap_or(false))
                            .collect(),
                        scroll_px: e.scroll_px,
                        drag: self.row_drag,
                        can_undo: !e.undo.is_empty(),
                        sbar_active: self.sbar_active(),
                    },
                ),
                None => c.fill(theme.bg),
            },
            Screen::PlaylistCover => match self.playlist_row() {
                Some(pl) => crate::playlist_cover::render(
                    c,
                    &theme,
                    fonts,
                    &self.lib,
                    pl,
                    &self.cover_choices(),
                    self.cover_scroll_px,
                    self.sbar_active(),
                ),
                None => c.fill(theme.bg),
            },
            Screen::ViewEdit => match self.view_edit.as_ref() {
                Some((at, draft)) => crate::view_edit::render(
                    c,
                    &theme,
                    fonts,
                    &crate::view_edit::ViewEditView {
                        draft,
                        matches: draft.tracks(&self.lib, self.clock_epoch).len(),
                        existing: at.is_some(),
                    },
                ),
                None => c.fill(theme.bg),
            },
            Screen::PlaylistPick => {
                let targets = self.user_playlists();
                let rows: Vec<crate::playlist_pick::Target> = targets
                    .iter()
                    .map(|(_, name, tracks)| crate::playlist_pick::Target {
                        name,
                        tracks: *tracks,
                    })
                    .collect();
                let sony = self
                    .lib
                    .playlists
                    .iter()
                    .filter(|p| !p.user && !p.smart)
                    .count();
                let track = if self.pick_many.is_empty() {
                    self.lib
                        .songs
                        .iter()
                        .find(|s| s.object_id == self.pick_track)
                        .map(|s| s.title.clone())
                        .unwrap_or_default()
                } else {
                    // The count only: a folder name with it was cut at the caption's width (11-23),
                    // and the name is already the new playlist's suggested name.
                    crate::ui_format!("{n} songs", n = self.pick_many.len())
                };
                crate::playlist_pick::render_targets(
                    c,
                    &theme,
                    fonts,
                    crate::i18n::text("Add to playlist"),
                    &track,
                    !self.pick_many.is_empty(),
                    &rows,
                    sony,
                    self.pick_scroll_px,
                    self.sbar_active(),
                )
            }
            Screen::TrackPick => {
                let songs = self.track_pick_songs();
                let (name, members) = self
                    .playlist_row()
                    .map(|p| {
                        (
                            p.name.clone(),
                            p.track_list.iter().map(|s| s.object_id).collect::<Vec<_>>(),
                        )
                    })
                    .unwrap_or_default();
                let is_in = |row: usize| {
                    songs
                        .get(row)
                        .map(|s| members.contains(&s.object_id))
                        .unwrap_or(false)
                };
                crate::playlist_pick::render_tracks(
                    c,
                    &theme,
                    fonts,
                    &name,
                    &songs,
                    &is_in,
                    self.track_pick_scroll_px,
                    members.len(),
                    &self.track_pick_query,
                    self.lib.songs.len(),
                    self.sbar_active(),
                )
            }
            Screen::Search => {
                let rows = self.search_rows();
                crate::search::render(
                    c,
                    &theme,
                    fonts,
                    &rows,
                    &self.search_query,
                    self.lib.songs.len(),
                    self.search_scroll_px,
                    self.sbar_active(),
                )
            }
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
        // covers it. Onboarding owns the whole panel, and the Winamp and cassette views draw their
        // own top edge, so those three go without; every other screen has it (the owner, 11-01).
        // USB storage shows it for the clock and battery, but its taps stay with the modal: a
        // stray tap must not take the drive away from a PC mid-copy.
        // A screen that leaves its scroll clip set would hide the bar (Storage did, 10-26).
        c.clear_clip();
        c.clear_clip_x();
        if self.current() != Screen::Onboarding && !self.landscape() {
            // The pull-down hangs from the bar's bottom edge; Big view's page titles reach up into
            // the bar, and their feet showed between it and the panel. Clear the band first.
            if self.quick_open {
                crate::widgets::fill_rect(
                    c,
                    0,
                    0,
                    crate::canvas::W as i32,
                    crate::chrome::STATUS_H,
                    theme.bg,
                );
            }
            let busy = self.busy().then_some(self.busy_phase);
            crate::chrome::status_bar(c, &theme, fonts, np.clock, np.badge, np.battery, busy);
        }

        // Now Playing return bar, pinned to the bottom of the browsing screens. Drawn after the
        // screen (it overlays nothing — `library::LIST_BOTTOM` stops above it) and before the
        // transient HUDs, which must stay on top of everything.
        if Self::shows_np_bar(self.current()) {
            crate::chrome::np_bar(
                c,
                &theme,
                fonts,
                np.title,
                np.artist,
                self.playing,
                np.progress,
            );
        }
        // Swipe-to-queue chip riding the flicked row (list screens only — if the user navigates
        // away mid-animation the anchor row is gone, so it just stops). It is anchored to a ROW,
        // so it belongs with the screen: UNDER the Shelf sheet, unlike the transients below.
        if self.queue_anim_frames > 0
            && matches!(
                self.current(),
                Screen::Library | Screen::Album | Screen::Artist | Screen::Playlist
            )
        {
            let p = self.queue_anim_frames as f32 / QUEUE_ANIM_FRAMES as f32;
            crate::overlay::queue_chip(c, &theme, fonts, self.queue_anim_y, p);
        }
        // Shelf bottom-sheet: dims the screen behind + draws the sheet over the lower half.
        if self.shelf_open {
            let (title, sub) = self.place_label();
            let pins: [Option<crate::shelf::Pin>; crate::shelf::SLOTS] = std::array::from_fn(|i| {
                self.pins[i].as_ref().map(|p| crate::shelf::Pin {
                    title: &p.title,
                    sub: &p.sub,
                })
            });
            crate::shelf::render(c, &theme, fonts, &title, &sub, &pins);
        }
        // The pull-down panel hangs from the bottom of the status strip (drawn above), so the clock
        // and battery stay in view over it, and dims the rest of the screen.
        if self.quick_open {
            let dev = self.bt_connected.as_deref();
            let wifi_sub = self.menu_subtitles().wifi;
            crate::quick::render(
                c,
                &theme,
                fonts,
                &crate::quick::QuickView {
                    brightness: self.brightness,
                    bt_on: self.bt_on,
                    bt_device: dev,
                    night: self.night,
                    sleep_idx: self.sleep_idx,
                    // Busy shows as on, as on the Wi-Fi page: the switch moved when it was tapped.
                    wifi_on: !matches!(self.wifi_status.state, crate::wifi::State::Off),
                    wifi_sub: &wifi_sub,
                    icons: self.opt(Opt::MenuIcons),
                },
            );
        }
        // TRANSIENTS LAST — above the Shelf sheet too. Drawn before it, the sheet (which fills
        // y 406..800 opaquely) painted straight over both: the pin/clear confirmation was
        // invisible in the one place it fires, and the volume HUD — which still responds to Vol±
        // while the sheet is open — came up half-covered.
        // Over a full-panel view the HUD would cover it (and Winamp has its own slider).
        let wa = self.landscape();
        if self.vol_overlay > 0 && self.current() != Screen::Lock && !wa {
            if self.volume_hud == 1 {
                crate::overlay::volume_minimal(c, &theme, self.display_volume());
            } else {
                crate::overlay::volume_trimmed(
                    c,
                    &theme,
                    fonts,
                    self.display_volume(),
                    if self.bt_route { self.bt_trim } else { 0 },
                    self.bt_route,
                );
            }
        }
        if self.toast_frames > 0 && self.current() != Screen::Lock {
            crate::overlay::toast(c, &theme, fonts, &self.toast_for_render());
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
    /// How long the UI trusts its own play/pause guess before deferring to the player.
    ///
    /// Long enough to cover the round trip (the shell pauses/resumes and the next poll reports it),
    /// short enough that a guess which turned out wrong corrects itself while the user is still
    /// looking at the bar.
    const PLAYING_HOLD_MS: u32 = 900;

    /// Flip the transport state OPTIMISTICALLY, so the button responds under the finger instead of
    /// waiting out a poll, and start a hold during which the player's own answer is ignored.
    ///
    /// `self.playing` is what the mini-player bar draws, and it used to be a purely local flag that
    /// nothing ever reconciled: five places flipped it, and the real transport state was never read
    /// back. So anything that changed playback WITHOUT a tap — a track ending, a play that failed,
    /// a pause for USB-MSC — left the bar showing the opposite of the truth until the user happened
    /// to tap it twice. Reported 2026-08-19 as the library's play/pause "not accurate".
    fn set_playing_optimistic(&mut self, v: bool) {
        self.playing = v;
        self.playing_hold_ms = Self::PLAYING_HOLD_MS;
    }

    /// Adopt the player's transport state once the optimistic hold has run out. Called from
    /// `render`, which is the only place that sees the shell's `NowPlaying`.
    fn reconcile_playing(&mut self, np_playing: bool) {
        if self.playing_hold_ms == 0 {
            self.playing = np_playing;
        }
    }

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
        let dt = dt_ms.clamp(1, 200) as f32;
        // Run the optimistic play/pause hold down in REAL time, like everything else here.
        self.playing_hold_ms = self.playing_hold_ms.saturating_sub(dt_ms);
        // A connect that has spun this long went nowhere the shell could see: free the row so it can
        // be tapped again.
        if self.bt_connecting_name.is_some() {
            self.bt_connecting_ms = self.bt_connecting_ms.saturating_add(dt_ms);
            if self.bt_connecting_ms >= Self::BT_CONNECT_SPIN_MS {
                self.clear_bt_connecting();
            }
        }
        // Bluetooth busy spinner. Only runs while a connect attempt or a scan is genuinely in
        // flight, so an idle Devices screen costs nothing and repaints nothing. Wrapped at 8s (one
        // whole number of 8-dot revolutions) to keep the f32 exact forever.
        if self.busy() {
            // The ring has eight positions a second and the status bar draws it on every screen,
            // so a repaint is due only when its head moves: not every frame, which held most of a
            // processor for the whole of a 30 s connect or scan (11-10).
            let head = (self.busy_phase * 8.0) as i32;
            self.busy_phase = (self.busy_phase + dt / 1000.0) % 8.0;
            animating = (self.busy_phase * 8.0) as i32 != head;
        } else if self.busy_phase != 0.0 {
            self.busy_phase = 0.0; // one last repaint clears the spinner
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
                Screen::Sound => a.sound_scroll_px,
                Screen::Advanced => a.adv_scroll_px,
                Screen::Presets => a.presets_scroll_px,
                Screen::NetRadio => a.radio_scroll_px,
                Screen::Device => a.device_scroll_px,
                Screen::Storage => a.storage_scroll_px,
                Screen::Display => a.display_scroll(),
                Screen::Menu => a.menu_scroll(),
                Screen::BtCodec => a.bt_codec_scroll(),
                Screen::UpNext => a.queue_scroll_px,
                Screen::Lyrics => a.lyrics_scroll_px,
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
                self.swipe_row = if dx.abs() < 2 {
                    None
                } else {
                    Some(crate::library::SwipeRow { dx, ..s })
                };
                animating = true;
            }
        }
        // Queue reorder: hold the row near an edge and the list scrolls under it, so a track can
        // be moved further than one screenful. Time-based like everything else here, and driven
        // from the tick rather than from touch events — a finger held perfectly still delivers no
        // events at all, which is exactly when this has to keep working.
        if let Some(mut d) = self.row_drag {
            const EDGE_PX: i32 = 70;
            const EDGE_RATE: f32 = 520.0; // px/s at the very edge, tapering to 0 at EDGE_PX in
            let editing = self.current() == Screen::PlaylistEdit;
            let (top, bot) = if editing {
                (
                    crate::playlist_edit::LIST_TOP,
                    crate::playlist_edit::LIST_BOTTOM,
                )
            } else {
                let top = crate::chrome::HEADER_BOTTOM;
                (top, top + crate::up_next::queue_view_h())
            };
            let into = if d.y < top + EDGE_PX {
                -(top + EDGE_PX - d.y) as f32 / EDGE_PX as f32
            } else if d.y > bot - EDGE_PX {
                (d.y - (bot - EDGE_PX)) as f32 / EDGE_PX as f32
            } else {
                0.0
            };
            if into != 0.0 && editing {
                // The playlist editor's flat list: scroll under the held row, and re-ask where it
                // would land.
                if let Some(e) = self.pl_edit.as_mut() {
                    let step = (into.clamp(-1.0, 1.0) * EDGE_RATE * dt / 1000.0) as i32;
                    e.scroll_px = (e.scroll_px + step)
                        .clamp(0, crate::playlist_edit::max_scroll(e.order.len()));
                    d.to =
                        crate::playlist_edit::slot_for(e.order.len(), d.float_top(), e.scroll_px);
                    self.row_drag = Some(d);
                }
            } else if into != 0.0 {
                // Both of these read the UNIFIED layout: the edge-scroll clamps against the whole
                // list (history + current + queue + album), and the landing slot is measured over
                // the whole movable span.
                //
                // That last part was a live bug until the span existed: this line asked
                // `queue_slot_for` whatever list the row had been lifted from, so edge-scrolling
                // an album row re-measured its landing slot against the QUEUE section's top and
                // its length. Dragging an upcoming row to the bottom of the screen therefore
                // walked `to` somewhere unrelated to the finger. One span, one question, and the
                // mismatch has nowhere left to live.
                let l = self.up_next_layout();
                let step = (into.clamp(-1.0, 1.0) * EDGE_RATE * dt / 1000.0) as i32;
                self.queue_scroll_px = (self.queue_scroll_px + step).clamp(0, l.max_scroll_px());
                // The finger hasn't moved, but the content under it has — so where the row would
                // land has changed and the parted list must follow.
                d.to = l.movable_slot_for(d.from, d.float_top(), self.queue_scroll_px);
                self.row_drag = Some(d);
            }
            animating = true; // the lifted row is live; keep painting it
        }
        // HUD/toast countdowns are expressed in 60 fps frames; burn the number of frames that
        // really elapsed so their on-screen duration is the same at any frame rate.
        let frames = ((dt / FRAME_MS as f32).round() as u8).max(1);
        for ctr in [
            &mut self.vol_overlay,
            &mut self.toast_frames,
            &mut self.queue_anim_frames,
        ] {
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

    /// The volume limit's on/off. Off by default: a cap the user did not ask for, silently
    /// refusing to go louder, is indistinguishable from a broken volume rocker.
    /// Settings ▸ Ignore "The" in artists.
    pub fn ignore_the(&self) -> bool {
        self.ignore_the
    }

    /// Set Ignore "The" in artists and re-sort the artist lists under it at once. The Artists tab
    /// and the Albums tab's groups are STORED in order, so a flip that only changed the flag would
    /// draw the old order under the new rail letters.
    ///
    /// The artist and album pages hold indices into exactly those lists, so what they point at is
    /// kept by name and album id, not by position.
    pub fn set_ignore_the(&mut self, on: bool) {
        if self.ignore_the == on && self.lib.ignore_the == on {
            return;
        }
        let artist = self.artist_name_at(self.artist_view).map(str::to_string);
        let album = self
            .lib
            .albums_flat()
            .get(self.album_view)
            .map(|a| a.album_id);
        self.ignore_the = on;
        self.lib.ignore_the = on;
        self.lib.prepare_order();
        if let Some(i) = artist.and_then(|n| self.lib.artists.iter().position(|r| r.name == n)) {
            self.artist_view = i;
        }
        if let Some(i) =
            album.and_then(|id| self.lib.albums_flat().iter().position(|a| a.album_id == id))
        {
            self.album_view = i;
        }
        self.album_expanded = None; // a flat index too, and an accordion is cheap to reopen
        self.az_memo = None;
    }

    pub fn volume_limit(&self) -> bool {
        self.volume_limit
    }

    /// The volume readout style, as its settings-file word: `full` or `minimal`.
    pub fn volume_hud(&self) -> &'static str {
        if self.volume_hud == 1 {
            "minimal"
        } else {
            "full"
        }
    }
    /// Set the volume readout from its settings-file word. Anything else is Full, the default.
    /// `style=` in `cinder_settings.conf`.
    pub fn style_token(&self) -> &'static str {
        self.style.token()
    }

    /// An unknown word keeps the style as it was (see `Style::from_token`).
    pub fn set_style_token(&mut self, v: &str) {
        if let Some(s) = crate::style::Style::from_token(v) {
            self.style = s;
        }
    }

    pub fn style(&self) -> crate::style::Style {
        self.style
    }

    pub fn set_style(&mut self, s: crate::style::Style) {
        self.style = s;
    }

    pub fn set_volume_hud(&mut self, v: &str) {
        self.volume_hud = u8::from(v.trim() == "minimal");
    }
    pub fn set_volume_limit(&mut self, on: bool) {
        self.volume_limit = on;
    }
    pub fn toggle_volume_limit(&mut self) {
        self.volume_limit = !self.volume_limit;
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

    /// Move whichever volume the rocker currently owns, and report whether the ROUTE's own volume
    /// has to move — i.e. whether the shell should send an AVRCP step / write the mixer.
    ///
    /// It can be false only on Bluetooth with fine volume on, where a press is absorbed by the
    /// source-side trim instead of being sent to the sink. The shell then has nothing to do but
    /// re-apply the EQ, which it picks up from `bt_trim_half_db` on its next tick.
    fn vol_step(&mut self, up: bool) -> bool {
        if !self.bt_route && self.line_out_holds() {
            return false;
        }
        if !self.bt_route {
            self.volume = if up {
                (self.volume + 1).min(crate::overlay::VOL_MAX)
            } else {
                self.volume.saturating_sub(1)
            };
            return true;
        }
        let sub = self.bt_fine_substeps();
        if sub > 0 {
            // Down: deepen the trim until it has spent one AVRCP step's worth, THEN step the sink
            // and release the trim. Up: the mirror. The trim is always in (-sub, 0], so the two
            // borrow points are the only places the sink is touched, and every other press is a
            // half-dB move that costs no Bluetooth traffic at all.
            if up {
                if self.bt_trim < 0 {
                    self.bt_trim += 1;
                    return false;
                }
                if self.bt_volume < crate::overlay::BT_VOL_MAX {
                    self.bt_volume += 1;
                    self.bt_trim = -(sub - 1);
                    return true;
                }
                return false; // already at the sink's top: nothing to borrow from
            }
            if self.bt_trim > -(sub - 1) {
                self.bt_trim -= 1;
                return false;
            }
            if self.bt_volume > 0 {
                self.bt_volume -= 1;
                self.bt_trim = 0;
                return true;
            }
            // At the sink's floor the trim is the only thing left, so let it run to full depth
            // rather than refusing the press.
            if self.bt_trim > -sub {
                self.bt_trim -= 1;
            }
            return false;
        }
        self.bt_volume = if up {
            (self.bt_volume + 1).min(crate::overlay::BT_VOL_MAX)
        } else {
            self.bt_volume.saturating_sub(1)
        };
        true
    }

    /// How many half-dB sub-steps one AVRCP step is divided into, limited by the EQ headroom the
    /// user's own curve leaves.
    ///
    /// The trim is applied on top of the EQ curve, and a band that is already at the service's
    /// floor cannot go lower — it would clamp, and the curve would tilt as the volume moved. So the
    /// deepest band decides: with a flat EQ there is the full ±10 dB to work in, and with a band
    /// parked at -10 dB there is none and the rocker quietly goes back to whole AVRCP steps.
    fn bt_fine_substeps(&self) -> i8 {
        if self.bt_fine == 0 {
            return 0;
        }
        let want = (self.bt_fine as i8) * 2; // dB span -> half-dB steps
        let headroom = crate::eq::BAND_MAX + self.eq_bands.iter().copied().min().unwrap_or(0);
        want.min(headroom).max(0)
    }

    /// The Bluetooth fine-volume span in dB (0 = off), and its cycle.
    pub fn bt_fine_span(&self) -> u8 {
        self.bt_fine
    }
    pub fn set_bt_fine_span(&mut self, n: u8) {
        self.bt_fine = n.min(3);
        if self.bt_fine == 0 {
            self.bt_trim = 0;
        }
    }
    pub fn cycle_bt_fine(&mut self) {
        self.set_bt_fine_span((self.bt_fine + 1) % 4);
    }
    /// The source-side trim in half-dB units (<= 0). The shell adds this to every EQ band.
    pub fn bt_trim_half_db(&self) -> i8 {
        self.bt_trim
    }

    /// The level the HUD draws, always on the 0..VOL_MAX scale so the bar looks the same on both
    /// routes. Bluetooth's 30 steps are stretched over it; the number under the bar is therefore
    /// the 0..120 equivalent, not the raw AVRCP step.
    pub fn display_volume(&self) -> u8 {
        if self.bt_route {
            // The trim is part of what you HEAR, so it is part of what the bar shows — otherwise
            // three of every four presses would move the volume and not the bar, which reads as a
            // rocker that misses presses. One AVRCP step is BT_VOL_MAX/VOL_MAX of the bar; the
            // trim moves it by the fraction of a step it represents.
            let sub = self.bt_fine_substeps();
            let frac = if sub > 0 {
                self.bt_trim as i32 * 1_000 / sub as i32
            } else {
                0
            };
            let scaled = (self.bt_volume as i32 * 1_000 + frac) * crate::overlay::VOL_MAX as i32
                / (crate::overlay::BT_VOL_MAX as i32 * 1_000);
            let scaled = scaled.clamp(0, crate::overlay::VOL_MAX as i32) as u8;
            // Never let a live sink level round down to 0. The scale is 127 -> 120, so AVRCP 1
            // truncates to 0 — and 0 is the MIN readout, which would claim the sink is at its
            // floor when it is a step above it. One is the smallest honest answer.
            if scaled == 0 && self.bt_volume > 0 {
                1
            } else {
                scaled
            }
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
        // A codec belongs to a LINK. When the peer goes away or changes, the last one we were told
        // about stops being true — and a stale "CONNECTED · LDAC" on the next device would be a
        // confident lie, which is the failure mode this screen was built to stop.
        if next.is_none() || next != self.bt_connected {
            self.bt_codec_negotiated = 0;
        }
        if next.is_some() && next == self.bt_connecting_name {
            self.clear_bt_connecting(); // the device being connected is the one that linked
        }
        // THE ROWS FOLLOW THE LINK, not the last list read. A row's `connected` flag decides what a
        // tap on it does, and the shell only re-reads the list on entry and after a connect — so on
        // 2026-09-16 the headphones' row still said connected after a disconnect, and eleven taps
        // meant as "connect" each sent another hang-up. A row stays connected only while the live
        // peer has its name; a row the list did not mark is marked when its name links, unless
        // another row of the same name already holds the flag.
        let held = self
            .bt_paired
            .iter()
            .any(|d| d.connected && next.as_deref() == Some(&d.name));
        let mut marked = held;
        for d in &mut self.bt_paired {
            let named = next.as_deref() == Some(&d.name);
            if !named {
                d.connected = false;
            } else if !marked {
                d.connected = true;
                marked = true;
            }
        }
        self.bt_connected = next;
        self.bt_link_known = true;
        changed
    }

    // ── A/B sound setups = the two sound PROFILES ──────────────────────────────────────────
    /// The live setup, as a value.
    pub fn setup(&self) -> SoundSetup {
        SoundSetup {
            dsee: self.snd_dsee,
            vinyl: self.snd_vinyl,
            vpt: self.snd_vpt,
            vpt_mode: self.snd_vpt_mode,
            dc: self.snd_dc,
            dc_type: self.snd_dc_type,
            norm: self.snd_norm,
            clear: self.snd_clear,
            balance: self.snd_balance,
            eq_preset: self.eq_preset,
            eq_bands: self.eq_bands,
            src_direct: self.adv_source_direct,
            clear_phase: self.adv_clear_phase,
            dsee_ai: self.adv_dsee_ai,
            dsee_custom: self.adv_dsee_custom,
            dsee_mode: self.adv_dsee_mode,
            vinyl_type: self.adv_vinyl_type,
            tone: self.adv_tone,
            tone_bands: self.tone_bands,
            mono: self.mono,
            linear_amp: self.adv_hp_linear,
            dac_eq: self.dac_eq,
        }
    }

    /// Make `s` the live setup. Every value is clamped on the way in: a profile can arrive from a
    /// PC-writable settings file, and several of these reach Sony enums or gain ranges that do not
    /// clamp for themselves (an out-of-range tone band is ZEROED by the service, not clamped).
    pub fn set_setup(&mut self, s: SoundSetup) {
        self.snd_dsee = s.dsee;
        self.snd_vinyl = s.vinyl;
        self.snd_vpt = s.vpt;
        self.snd_vpt_mode = s.vpt_mode.min(VPT_MODES.len() - 1);
        self.snd_dc_type = s.dc_type.min(DC_PHASE_TYPES.len() - 1);
        self.snd_dc = s.dc;
        self.snd_norm = s.norm;
        self.snd_clear = s.clear;
        self.snd_balance = s.balance.min(crate::sound::BALANCE_MAX);
        self.eq_preset = s.eq_preset.min(crate::data::EQ_PRESETS.len() - 1);
        self.eq_bands = s.eq_bands;
        self.adv_source_direct = s.src_direct;
        self.adv_clear_phase = s.clear_phase;
        self.adv_dsee_ai = s.dsee_ai;
        self.adv_dsee_custom = s.dsee_custom;
        self.adv_dsee_mode = s.dsee_mode.min(crate::advanced::DSEE_MODES.len() - 1);
        self.adv_vinyl_type = s.vinyl_type.min(crate::advanced::VINYL_TYPES.len() - 1);
        self.adv_tone = s.tone;
        self.set_tone_bands(s.tone_bands);
        // The optional members. Unconditional: for one that is not followed, the caller has
        // already made `s` carry the live value (`share_unfollowed`).
        self.mono = s.mono;
        self.adv_hp_linear = s.linear_amp;
        self.set_dac_eq(s.dac_eq);
    }

    /// Which setup is live: 0 = A, 1 = B.
    pub fn setup_idx(&self) -> usize {
        self.setup_idx
    }

    /// The one that is not live — persisted so both survive a reboot.
    pub fn setup_inactive(&self) -> SoundSetup {
        self.setup_other
    }

    /// Restore both halves at boot. `idx` names which of the two is live.
    pub fn restore_setups(&mut self, live: SoundSetup, mut other: SoundSetup, idx: usize) {
        self.set_setup(live);
        other.share_unfollowed(&self.setup(), self.profile_follow);
        self.setup_other = other;
        self.setup_idx = idx & 1;
    }

    /// Make setup `idx` live, banking the current one. No toast, no action: the callers decide
    /// what to say and what the shell has to do. Returns true if anything changed.
    fn swap_to_setup(&mut self, idx: usize) -> bool {
        let idx = idx & 1;
        if idx == self.setup_idx {
            return false;
        }
        let live = self.setup();
        let mut next = self.setup_other;
        next.share_unfollowed(&live, self.profile_follow);
        self.setup_other = live;
        self.setup_idx = idx;
        self.set_setup(next);
        true
    }

    /// Switch to side `idx` of the compare — the Sound screen's A/B control. The live side is
    /// banked first, so edits are never lost by comparing. Returns the actions needed to make the
    /// hardware match — the whole chain, because everything in a setup can differ.
    pub fn select_setup(&mut self, idx: usize) -> Vec<Action> {
        let idx = idx & 1;
        if !self.swap_to_setup(idx) {
            return vec![];
        }
        self.notify_format(
            "Comparing {}",
            vec![("0", crate::profile::letter(idx).to_string())],
        );
        // ONE action, and it has to be SoundChanged. The FFI hands the shell a single code per tap
        // or press — the first shell-visible action — so the old `[EqChanged, SoundChanged]` only
        // ever reached the shell as EqChanged: switching setups moved the EQ and left DSEE, VPT,
        // DC Phase, Vinyl, the Normalizer, ClearAudio+ and the balance exactly where they were
        // (reported 2026-09-17: "A/B doesn't actually change between two presets"). The shell's
        // SoundChanged now re-applies the EQ as well, so this one action carries the whole setup.
        vec![Action::SoundChanged]
    }

    // ── profiles per output (R5) ─────────────────────────────────────────────────────────────
    /// The output that is live, from what the shell has reported.
    pub fn current_output(&self) -> crate::profile::Output {
        crate::profile::output_for(self.usb_dac_on, self.bt_route)
    }

    /// Which output the live setup was last chosen for.
    pub fn live_output(&self) -> crate::profile::Output {
        self.live_output
    }

    /// Which optional members travel with a profile (`FOLLOW_*` bits).
    pub fn profile_follow(&self) -> u8 {
        self.profile_follow
    }

    /// Set at boot from the settings file, BEFORE the profiles are restored.
    pub fn set_profile_follow(&mut self, bits: u8) {
        self.profile_follow = bits & FOLLOW_ALL;
    }

    /// The route may have changed. Called from every place the shell reports a route
    /// (`set_bt_route`, `set_usb_dac`) and from the USB-DAC switch itself. Moving on or off the
    /// jack moves the curve between the DAC and the 10-band (`jack_eq_dac`), so `profile_apply` is
    /// raised for the shell to collect with [`Self::take_profile_apply`].
    fn route_changed(&mut self) {
        let out = self.current_output();
        if out == self.live_output {
            return;
        }
        let jack = crate::profile::Output::Jack;
        if self.jack_eq_dac && (self.live_output == jack || out == jack) {
            self.profile_apply = true;
        }
        self.live_output = out;
    }

    /// Has a route change moved the jack's EQ since the last call? Take-once: the shell
    /// re-applies the chain when this returns true.
    pub fn take_profile_apply(&mut self) -> bool {
        std::mem::take(&mut self.profile_apply)
    }

    /// A tap on the Presets screen.
    fn presets_tap(&mut self, h: crate::presets::Hit) -> Vec<Action> {
        use crate::presets::Hit;
        if !matches!(h, Hit::Delete(_)) {
            self.preset_armed = None;
        }
        match h {
            // The keyboard names it first; DONE saves (`preset_named`), Back saves nothing.
            Hit::Save => {
                if self.presets.len() >= crate::presets::MAX {
                    self.notify_format(
                        "All {} are in use: delete one first",
                        vec![("0", crate::presets::MAX.to_string())],
                    );
                    return vec![];
                }
                self.open_keyboard(KbPurpose::PresetName(None));
                vec![]
            }
            Hit::Rename(i) => {
                if i < self.presets.len() {
                    self.open_keyboard(KbPurpose::PresetName(Some(i)));
                }
                vec![]
            }
            Hit::Load(i) => {
                let Some(mut p) = self.presets.get(i).copied() else {
                    return vec![];
                };
                p.share_unfollowed(&self.setup(), self.profile_follow);
                self.set_setup(p);
                self.notify_format(
                    "{} on {}",
                    vec![
                        ("0", crate::presets::name(&self.preset_names, i)),
                        ("1", crate::profile::letter(self.setup_idx).to_string()),
                    ],
                );
                vec![Action::SoundChanged]
            }
            Hit::Delete(i) => {
                if self.preset_armed == Some(i) && i < self.presets.len() {
                    let name = crate::presets::name(&self.preset_names, i);
                    self.presets.remove(i);
                    if i < self.preset_names.len() {
                        self.preset_names.remove(i);
                    }
                    let gone = (i + 1) as u8;
                    self.album_sounds.retain(|(_, n)| *n != gone);
                    for (_, n) in &mut self.album_sounds {
                        if *n > gone {
                            *n -= 1;
                        }
                    }
                    self.preset_armed = None;
                    self.presets_scroll_px = self
                        .presets_scroll_px
                        .min(crate::presets::max_scroll(self.presets.len()));
                    self.notify_format("{} deleted", vec![("0", name)]);
                } else {
                    self.preset_armed = Some(i);
                }
                vec![]
            }
            // Both sides start from the value that is live, so switching an item in changes
            // nothing that is heard; switching it out leaves the live value as the one for both.
            Hit::Follow(bit) => {
                self.profile_follow ^= bit & FOLLOW_ALL;
                let live = self.setup();
                self.setup_other
                    .share_unfollowed(&live, self.profile_follow & !bit);
                vec![]
            }
            Hit::Copy => {
                self.setup_other = self.setup();
                let other = crate::profile::letter(1 - self.setup_idx);
                let live = crate::profile::letter(self.setup_idx);
                self.notify_format(
                    "{other} is now a copy of {live}",
                    vec![("other", other.to_string()), ("live", live.to_string())],
                );
                vec![]
            }
        }
    }

    /// The playing album as `(key, title)`. The key is album and album artist, not the DB's
    /// album id, which a rescan renumbers.
    fn playing_album(&self) -> Option<(String, String)> {
        let t = self.context.get(self.context_idx)?;
        Some((
            format!("{}\u{1f}{}", t.art, t.group_artist()),
            t.art.clone(),
        ))
    }

    /// The playing song's key among the album ones: a song entry is told from an album entry by
    /// its leading \u{1e}, so both share `album_sounds` and the `album_sound=` lines.
    fn playing_song_key(&self) -> Option<String> {
        let t = self.context.get(self.context_idx)?;
        Some(format!(
            "\u{1e}{}\u{1f}{}\u{1f}{}",
            t.title, t.artist, t.art
        ))
    }

    /// The preset saved for `key` (0 = none).
    fn sound_n(&self, key: &str) -> u8 {
        self.album_sounds
            .iter()
            .find(|(k, _)| k == key)
            .map_or(0, |(_, n)| *n)
    }

    /// The playing album's preset and the playing song's (0 = none), for Track info's band.
    fn album_sound_row(&self) -> Option<(u8, u8)> {
        let (album, _) = self.playing_album()?;
        let song = self.playing_song_key()?;
        Some((self.sound_n(&album), self.sound_n(&song)))
    }

    /// The preset that should be live for the playing track, and the entry it comes from: the
    /// song's when it has one, else the album's (0 = neither).
    fn sound_effective(&self) -> Option<(String, u8)> {
        let (album, _) = self.playing_album()?;
        let song = self.playing_song_key()?;
        let n = self.sound_n(&song);
        Some(if n > 0 {
            (song, n)
        } else {
            let n = self.sound_n(&album);
            (album, n)
        })
    }

    /// Make the effective preset live, or put back the sound from before. True when it changed.
    fn sound_apply(&mut self) -> bool {
        let Some((key, n)) = self.sound_effective() else {
            return false;
        };
        self.album_sound_key = Some(key);
        if n > 0 {
            self.album_sound_load(n)
        } else {
            self.album_sound_restore()
        }
    }

    /// Load preset `n` (1-based) as an album's sound, keeping what was live to put back after.
    fn album_sound_load(&mut self, n: u8) -> bool {
        let Some(mut p) = self.presets.get(n as usize - 1).copied() else {
            return false;
        };
        if self.album_sound_prev.is_none() {
            self.album_sound_prev = Some(self.setup());
        }
        p.share_unfollowed(&self.setup(), self.profile_follow);
        self.set_setup(p);
        true
    }

    /// Put back the sound from before an album's preset, if one took over.
    fn album_sound_restore(&mut self) -> bool {
        let Some(mut p) = self.album_sound_prev.take() else {
            return false;
        };
        p.share_unfollowed(&self.setup(), self.profile_follow);
        self.set_setup(p);
        true
    }

    /// A tap on Track info's ALBUM (`song` false) or SONG control: Off, Preset 1, Preset 2, … and
    /// back to Off, for the playing album or song. A song's preset wins over its album's.
    fn album_sound_step(&mut self, song: bool) -> Vec<Action> {
        let key = if song {
            self.playing_song_key()
        } else {
            self.playing_album().map(|(k, _)| k)
        };
        let Some(key) = key else {
            self.notify(crate::i18n::text("Nothing playing"));
            return vec![];
        };
        if self.presets.is_empty() {
            self.notify(crate::i18n::text(
                "Save a preset first (Sound \u{25b8} Presets)",
            ));
            return vec![];
        }
        let next = ((self.sound_n(&key) as usize + 1) % (self.presets.len() + 1)) as u8;
        self.album_sounds.retain(|(k, _)| *k != key);
        if next > 0 {
            self.album_sounds.push((key, next));
        }
        if self.sound_apply() {
            vec![Action::SoundChanged]
        } else {
            vec![]
        }
    }

    /// Called by the shell when the playing track changes. A song or album with a preset loads it;
    /// one without puts back the sound from before. Tracks that resolve to the same entry (an
    /// album's songs) change nothing. True when the sound changed: the shell collects that with
    /// [`Self::take_profile_apply`], which this raises.
    pub fn album_sound_follow(&mut self) -> bool {
        let Some((key, n)) = self.sound_effective() else {
            return false;
        };
        if self.album_sound_key.as_deref() == Some(key.as_str()) {
            return false;
        }
        let changed = self.sound_apply();
        if changed && n > 0 {
            let msg = if key.starts_with('\u{1e}') {
                "Preset {} for this song"
            } else {
                "Preset {} for this album"
            };
            self.notify_format(msg, vec![("0", n.to_string())]);
        }
        self.profile_apply |= changed;
        changed
    }

    /// The sound an album's preset replaced, for the settings file.
    pub fn album_sound_prev(&self) -> Option<&SoundSetup> {
        self.album_sound_prev.as_ref()
    }
    pub fn restore_album_sound_prev(&mut self, p: Option<SoundSetup>) {
        self.album_sound_prev = p;
    }

    /// Every album's preset, for the settings file.
    pub fn album_sounds(&self) -> &[(String, u8)] {
        &self.album_sounds
    }
    /// Restore the album presets at boot. Entries naming a preset that is not saved are dropped.
    pub fn restore_album_sounds(&mut self, mut v: Vec<(String, u8)>) {
        let n = self.presets.len() as u8;
        v.retain(|(_, p)| (1..=n).contains(p));
        self.album_sounds = v;
    }

    /// Saved presets, oldest first.
    pub fn presets(&self) -> &[SoundSetup] {
        &self.presets
    }
    /// Their names, by index; empty (or missing) for "Preset N".
    pub fn preset_names(&self) -> &[String] {
        &self.preset_names
    }
    /// Restore the saved presets and their names at boot (at most `presets::MAX`).
    pub fn restore_presets(&mut self, mut p: Vec<SoundSetup>, names: Vec<String>) {
        p.truncate(crate::presets::MAX);
        self.preset_names = names
            .iter()
            .take(p.len())
            .map(|n| crate::presets::clean_name(n))
            .collect();
        self.presets = p;
    }

    /// The keyboard's DONE on a preset name: save the live sound under it (`at` None), or rename.
    /// A name left as "Preset N" is stored empty, so it follows the number when one above goes.
    fn preset_named(&mut self, at: Option<usize>) -> Vec<Action> {
        let i = at.unwrap_or(self.presets.len());
        let mut name = crate::presets::clean_name(&self.kb_text);
        if name == crate::presets::name(&[], i) {
            name.clear();
        }
        match at {
            None => {
                if self.presets.len() >= crate::presets::MAX {
                    return vec![];
                }
                self.presets.push(self.setup());
                self.preset_names
                    .resize(self.presets.len() - 1, String::new());
                self.preset_names.push(name);
                self.notify_format(
                    "Saved as {}",
                    vec![("0", crate::presets::name(&self.preset_names, i))],
                );
            }
            Some(i) if i < self.presets.len() => {
                self.preset_names.resize(self.presets.len(), String::new());
                self.preset_names[i] = name;
            }
            Some(_) => {}
        }
        vec![]
    }

    /// The shell's report of what is in the jack. True when it changed.
    pub fn set_nc_plug(&mut self, plug: bool, ambient_ok: bool) -> bool {
        let changed = self.nc_plug != plug || self.nc_ambient_ok != ambient_ok;
        self.nc_plug = plug;
        self.nc_ambient_ok = ambient_ok;
        if !plug && self.current() == Screen::Nc {
            self.pop();
        }
        changed
    }
    /// The `(noise cancel mode, noise cancel headphone type)` items to write, while the
    /// earphones are in. A mode the earphones cannot run is written as `off`.
    pub fn nc_items(&self) -> Option<(&'static str, &'static str)> {
        let m = if crate::nc::mode_ok(self.nc_mode, self.nc_model, self.nc_ambient_ok) {
            self.nc_mode
        } else {
            crate::nc::MODE_OFF
        };
        self.nc_plug
            .then(|| (crate::nc::MODES[m].2, crate::nc::MODELS[self.nc_model].1))
    }
    pub fn nc_mode(&self) -> usize {
        self.nc_mode
    }
    pub fn nc_model(&self) -> usize {
        self.nc_model
    }
    pub fn set_nc(&mut self, mode: usize, model: usize) {
        self.nc_mode = mode.min(crate::nc::MODES.len() - 1);
        self.nc_model = model.min(crate::nc::MODELS.len() - 1);
    }
    /// The Sound list's scroll as `sound::` is given it: one row less while the Noise cancelling
    /// row sits above the list (see `sound::NC_H()`).
    fn sound_scroll(&self) -> i32 {
        self.sound_scroll_px
            - if self.nc_plug {
                crate::sound::NC_H()
            } else {
                0
            }
    }
    fn sound_max_scroll(&self) -> i32 {
        crate::sound::max_scroll()
            + if self.nc_plug {
                crate::sound::NC_H()
            } else {
                0
            }
    }

    /// The jack's EQ is the DAC's (see the field).
    pub fn jack_eq_dac(&self) -> bool {
        self.jack_eq_dac
    }
    pub fn set_jack_eq_dac(&mut self, on: bool) {
        self.jack_eq_dac = on;
    }
    /// The DAC is carrying the curve right now: the setting is on and the jack is the output.
    pub fn eq_on_dac(&self) -> bool {
        self.jack_eq_dac && self.current_output() == crate::profile::Output::Jack
    }
    /// What Sony's 10-band should be given: flat while the DAC carries the jack's curve.
    pub fn eq_bands_for_dsp(&self) -> [i8; 10] {
        if self.eq_on_dac() {
            [0; 10]
        } else {
            self.eq_bands
        }
    }

    /// Push the wall clock (UTC epoch seconds). The shell calls this from its ~1 Hz housekeeping.
    /// Ignored while the editor is open, so the per-second push cannot drag a field out from under
    /// the finger that is editing it.
    pub fn set_clock_epoch(&mut self, epoch: i64) {
        self.clock_epoch = epoch;
        if self.current() != Screen::ClockSet {
            self.clock_fields = crate::clockset::fields_from_epoch(epoch);
        }
    }

    /// The epoch the editor would write. Read by the shell after a `ClockSet` action.
    pub fn clock_epoch_pending(&self) -> i64 {
        crate::clockset::epoch_from_fields(&self.clock_fields)
    }

    /// Value for the Settings ▸ Date & time row: "17 Aug · 09:01". Falls back to the status-bar
    /// string until the shell has reported a real epoch — a confident "1 Jan 1970" would be worse
    /// than saying only what we actually know.
    fn clock_label(&self, fallback: &str) -> String {
        if self.clock_epoch <= 0 {
            return fallback.to_string();
        }
        let f = crate::clockset::fields_from_epoch(self.clock_epoch);
        let mon = crate::clockset::MONTHS[(f[1].clamp(1, 12) - 1) as usize];
        format!("{} {} · {:02}:{:02}", f[2], &mon[..3], f[3], f[4])
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
        // The row INDEX belonged to the old ordering; the device it was for is found again below.
        self.bt_connecting = None;
    }

    pub fn bt_paired_add(&mut self, name: &str, kind: &str, connected: bool) {
        self.bt_paired.push(crate::pairing::PairedDevice {
            name: name.to_string(),
            kind: kind.to_string(),
            connected,
        });
        // A connect in flight survives the list refresh the shell answers every connect with.
        if self.bt_connecting_name.as_deref() == Some(name) {
            if connected {
                self.clear_bt_connecting(); // it worked
            } else if self.bt_connecting.is_none() {
                self.bt_connecting = Some(self.bt_paired.len() - 1);
            }
        }
    }

    /// How long a connect spinner runs before the row is freed for another tap. The shell's retry
    /// carries on regardless; this only bounds how long the row refuses to ask again.
    const BT_CONNECT_SPIN_MS: u32 = 30_000;

    /// A tap on a paired row that is not the connected one.
    ///
    /// On 2026-09-15 the headphones' row was tapped 24 times in 21 s: the spinner a tap started
    /// was cleared by the paired-list refresh the shell answers every connect with, so it lasted a
    /// frame, nothing on screen said a connect was under way, and each tap sent the radio a fresh
    /// request while its own page was still on the air. Now the spinner survives the refresh (see
    /// `bt_paired_add`), and while it runs the same row asks for nothing more.
    fn bt_connect_row(&mut self, i: usize) -> Vec<Action> {
        let name = self.bt_paired.get(i).map(|d| d.name.clone());
        if self.bt_connecting == Some(i) && self.bt_connecting_name == name {
            return vec![];
        }
        self.bt_connecting = Some(i);
        self.bt_connecting_name = name;
        self.bt_connecting_ms = 0;
        vec![Action::BtConnectDevice(i)]
    }

    fn clear_bt_connecting(&mut self) {
        self.bt_connecting = None;
        self.bt_connecting_name = None;
        self.bt_connecting_ms = 0;
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
            Some(crate::pairing::Prompt {
                kind,
                name: name.to_string(),
                code,
            })
        };
        // The prompt is drawn only on Devices. A request the user did not start there — a PC or
        // phone pairing TO the Walkman — arrived while another screen was up, stayed invisible,
        // and the other side sat on its PIN until it gave up (2026-09-29, four tries from a PC).
        // The radio is blocked waiting for this answer, so go where it can be given.
        if kind != 0 && self.current() != Screen::Pairing && self.current() != Screen::Receiver {
            self.push(Screen::Pairing);
        }
    }

    /// The BT Receiver switch, as the shell should apply it: on only while the Receiver page is
    /// still in the stack. Leaving it — Back, an edge swipe, the Menu, a jump to Now Playing — has
    /// many routes and none of them can emit an action, so the shell's once-a-second receiver tick
    /// reads this and stops the mode when the page has gone.
    pub fn rx_on(&self) -> bool {
        self.rx_on && self.stack.contains(&Screen::Receiver)
    }

    /// Receiver codec preference (0 Auto, 1 LDAC, 2 AAC, 3 SBC), independent of transmitter.
    pub fn rx_codec_pref(&self) -> u8 {
        self.rx_codec_pref
    }

    /// Restore or select a supported receiver preference. Old files omit it and invalid values
    /// fall back to Auto, preserving the original all-codecs-enabled behavior.
    pub fn set_rx_codec_pref(&mut self, pref: u8) {
        self.rx_codec_pref = if pref <= 3 { pref } else { 0 };
    }

    /// What the shell read from BtPlayerService. `phase` 1 waiting, 2 attached, 3 audio arriving.
    pub fn set_rx_status(&mut self, phase: u8, peer: &str, codec: u32, freq: u32, bitrate: u32) {
        self.rx_phase = phase;
        self.rx_peer = peer.to_string();
        self.rx_codec = codec;
        self.rx_freq = freq;
        self.rx_bitrate = bitrate;
    }

    /// The shell polls the radio and pushes the answer here; the rocker follows it from the next
    /// press onward. Setting it does NOT touch either level — that is the whole point.
    pub fn set_bt_route(&mut self, on: bool) {
        if self.bt_route != on {
            // The trim is a Bluetooth-only attenuation applied through the GLOBAL EQ, so leaving it
            // in place while the jack takes over would quietly play the jack up to 3 dB down. The
            // shell re-applies the EQ when it sees the trim change.
            self.bt_trim = 0;
        }
        self.bt_route = on;
        self.route_changed();
    }

    /// The current 10-band EQ gains (dB), for the shell to apply to the device DSP.
    pub fn eq_bands(&self) -> [i8; 10] {
        self.eq_bands
    }
    /// Restore EQ gains (from persisted settings). Clamps each band to the range the EQ screen can
    /// actually reach.
    ///
    /// That range is `eq::BAND_MAX` — 20 half-dB units, i.e. ±10 dB, the same range the DSP
    /// honours. It was ±6 here, which is ±3 dB: a hardcoded number that did not match the editor
    /// above it or the service below it, so every band a user set past ±3 dB came back a third of
    /// its size on the next boot. The EQ screen has always been able to reach ±10 dB, and the
    /// service has always accepted it (`cinder_eq_clamp_gain`, same 20).
    pub fn set_eq_bands(&mut self, bands: [i8; 10]) {
        for (dst, src) in self.eq_bands.iter_mut().zip(bands.iter()) {
            *dst = (*src).clamp(-crate::eq::BAND_MAX, crate::eq::BAND_MAX);
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
    pub fn now_playing_queue<'a>(
        &'a self,
        title: &str,
        artist: &str,
    ) -> Option<(&'a str, &'a [SongRow], usize)> {
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
        if m == 0 && !self.stop_after {
            self.sleep_idx = 0; // expired/cancelled → back to "Off" in the cycle
        }
    }
    pub fn sleep_label(&self) -> String {
        if self.stop_after {
            crate::i18n::text("END OF SONG").to_string()
        } else if self.sleep_min == 0 {
            crate::i18n::text("OFF").to_string()
        } else {
            crate::ui_format!("{} MIN", self.sleep_min)
        }
    }

    /// Pick sleep preset `idx` (an index into `quick::SLEEP_PRESETS`) — from the Settings row or
    /// the pull-down panel, which share the list. "End of song" is not a countdown: it arms
    /// `stop_after` and cancels any running timer (`SleepTimer(0)`); the shell pauses at the end
    /// of the playing song and calls `stop_after_done`.
    fn set_sleep_choice(&mut self, idx: usize) -> Vec<Action> {
        use crate::quick::{SLEEP_END_OF_SONG, SLEEP_PRESETS};
        self.sleep_idx = idx.min(SLEEP_PRESETS.len() - 1);
        let preset = SLEEP_PRESETS[self.sleep_idx];
        self.stop_after = preset == SLEEP_END_OF_SONG;
        self.sleep_min = if self.stop_after { 0 } else { preset };
        vec![Action::SleepTimer(self.sleep_min)]
    }

    /// Is "stop after this song" armed? Read by the shell every frame while it times the end.
    pub fn stop_after(&self) -> bool {
        self.stop_after
    }
    /// The song ended and playback paused: back to "Off", as an expired timer goes.
    pub fn stop_after_done(&mut self) {
        self.stop_after = false;
        self.sleep_idx = 0;
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
            self.pop(); // replayed from Help & controls → back to it
        } else {
            self.go(Screen::NowPlaying); // first-run → start listening
            self.apply_home(); // …on the home screen, which is Library unless chosen otherwise
        }
    }

    /// Battery-care (Itawari) toggle state. The shell reads the device's real value at boot and
    /// pushes it via set_battery_care; battery_care() returns the UI's current desired value so the
    /// shell can apply a toggle to PowerMgrServiceClient.
    pub fn battery_care(&self) -> bool {
        self.battery_care
    }
    /// Flip battery care and emit the action the shell turns into an Itawari call. Shared by the
    /// Battery screen's touch handler and its Select press, so the two cannot diverge.
    fn battery_care_toggle(&mut self) -> Vec<Action> {
        self.battery_care = !self.battery_care;
        vec![Action::BatteryCareChanged(self.battery_care)]
    }

    /// Assemble the Device screen's view from the pushed readings. ONE builder, used by the
    /// render, the hit test and the scroll extent — those three must agree about what is on the
    /// screen, and the care footnote alone changes its height.
    fn device_view(&self) -> crate::device::DeviceView<'_> {
        crate::device::DeviceView {
            percent: self.batt_pct,
            status: &self.batt_status,
            health: &self.batt_health,
            millivolts: self.batt_mv,
            care: self.battery_care,
            chg_state: self.batt_chg_state,
            chg_fault: self.batt_chg_fault,
            charger_raw: &self.batt_raw,
            temp_cpu: self.dev_temp_cpu,
            temp_pmic: self.dev_temp_pmic,
            temp_abb: self.dev_temp_abb,
            cpu_khz: self.dev_cpu_khz,
            cpu_max_khz: self.dev_cpu_max_khz,
            cores_online: self.dev_cores_online,
            cores_total: self.dev_cores_total,
            governor: &self.dev_governor,
            mem_total_kb: self.dev_mem_total_kb,
            mem_avail_kb: self.dev_mem_avail_kb,
            music_total_mb: self.dev_music_total_mb,
            music_free_mb: self.dev_music_free_mb,
            data_free_mb: self.dev_data_free_mb,
            uptime_s: self.dev_uptime_s,
            kernel: &self.dev_kernel,
            firmware: crate::settings::FIRMWARE_LABEL,
            base_fw: &self.dev_base_fw,
        }
    }

    /// The Storage screen's view: the library's summary plus the pushed internal-storage reading.
    fn storage_view(&self) -> crate::storage::StorageView<'_> {
        crate::storage::StorageView {
            sum: &self.lib.summary,
            albums: self.lib.album_groups.iter().map(|g| g.albums.len()).sum(),
            artists: self.lib.artists.len(),
            genres: self.lib.genres.len(),
            int_total_mb: self.dev_music_total_mb,
            int_free_mb: self.dev_music_free_mb,
        }
    }

    /// A one-line summary for the Settings row that opens this screen, e.g. "99% · 34.4 °C".
    /// Built here rather than in the shell so the same formatters the screen uses produce it.
    pub fn device_summary(&self) -> String {
        let t = crate::device::temp_label(self.dev_temp_cpu);
        if t == "—" {
            crate::device::percent_label(self.batt_pct)
        } else {
            format!("{} · {}", crate::device::percent_label(self.batt_pct), t)
        }
    }

    /// Push the battery readouts the shell has just measured. Called on the shell's battery tick
    /// (~10 s), so the screen is live without the UI ever touching a file.
    #[allow(clippy::too_many_arguments)]
    pub fn set_battery_detail(
        &mut self,
        pct: u8,
        status: &str,
        health: &str,
        mv: i32,
        chg_state: i32,
        chg_fault: i32,
        raw: &str,
    ) {
        self.batt_pct = pct.min(100);
        self.batt_status.clear();
        self.batt_status.push_str(status);
        self.batt_health.clear();
        self.batt_health.push_str(health);
        self.batt_mv = mv;
        self.batt_chg_state = chg_state;
        self.batt_chg_fault = chg_fault;
        self.batt_raw.clear();
        self.batt_raw.push_str(raw);
    }

    /// The three die temperatures, in millidegrees C.
    pub fn set_device_temps(&mut self, cpu: i32, pmic: i32, abb: i32) {
        self.dev_temp_cpu = cpu;
        self.dev_temp_pmic = pmic;
        self.dev_temp_abb = abb;
    }

    /// Clock (kHz), maximum clock, cores online out of total, and the cpufreq governor.
    pub fn set_device_cpu(&mut self, khz: i32, max_khz: i32, online: i32, total: i32, gov: &str) {
        self.dev_cpu_khz = khz;
        self.dev_cpu_max_khz = max_khz;
        self.dev_cores_online = online;
        self.dev_cores_total = total;
        self.dev_governor.clear();
        self.dev_governor.push_str(gov);
    }

    /// Memory from /proc/meminfo (kB) and the two volumes that matter, in MB.
    pub fn set_device_storage(
        &mut self,
        mem_total_kb: i32,
        mem_avail_kb: i32,
        music_total_mb: i32,
        music_free_mb: i32,
        data_free_mb: i32,
    ) {
        self.dev_mem_total_kb = mem_total_kb;
        self.dev_mem_avail_kb = mem_avail_kb;
        self.dev_music_total_mb = music_total_mb;
        self.dev_music_free_mb = music_free_mb;
        self.dev_data_free_mb = data_free_mb;
    }

    /// The SD card's size and free space in MB, as the bridge reads them now (0, 0 = no card
    /// mounted). The library's own reading is taken when the library is built, which is before the
    /// card mounts on every boot, so the Storage screen had no SD card bar (2026-11-11). True when
    /// the numbers changed.
    pub fn set_sd_space(&mut self, total_mb: i32, free_mb: i32) -> bool {
        let s = &mut self.lib.summary;
        let changed = (s.sd_total_mb, s.sd_free_mb) != (total_mb, free_mb);
        (s.sd_total_mb, s.sd_free_mb) = (total_mb, free_mb);
        changed
    }

    /// Which firmware Cinder is installed on top of, as the shell detected it ("SONY STOCK",
    /// "WALKMAN ONE"). Pushed once — it cannot change without a reinstall.
    pub fn set_device_base_fw(&mut self, base: &str) {
        if self.dev_base_fw != base {
            self.dev_base_fw = base.to_string();
        }
    }

    /// Seconds since boot and the kernel release.
    pub fn set_device_system(&mut self, uptime_s: i32, kernel: &str) {
        self.dev_uptime_s = uptime_s;
        self.dev_kernel.clear();
        self.dev_kernel.push_str(kernel);
    }

    pub fn set_battery_care(&mut self, on: bool) {
        self.battery_care = on;
    }

    /// Sound effect toggles as a bitmask for the shell to apply via EffectCtrlDmp:
    /// bit0 DSEE · bit1 Vinyl · bit2 VPT · bit3 DC-Phase · bit4 Normalizer · bit5 ClearAudio+.
    /// L/R balance position, 0..=100 (50 = centre). Its own value rather than a bit in
    /// `sound_flags`, which is a u8 of booleans.
    pub fn balance(&self) -> usize {
        self.snd_balance
    }
    /// Snap a loaded value into range, so a hand-edited settings file cannot park the knob off the
    /// end of the track.
    pub fn set_balance(&mut self, i: usize) {
        self.snd_balance = i.min(crate::sound::BALANCE_MAX);
    }
    /// MONO (accessibility) — sum left and right. See the field, and `analysis/RE_mono_audio.md`
    /// for how far it reaches on this hardware.
    pub fn mono(&self) -> bool {
        self.mono
    }
    pub fn set_mono(&mut self, on: bool) {
        self.mono = on;
    }
    // ── Soundscapes ──────────────────────────────────────────────────────────────────────────────

    /// What the shell needs: `(sound id or 0 when off, level on its own %, level with music %)`.
    pub fn ambient(&self) -> (u8, u8, u8) {
        (
            if self.ambient_on {
                self.ambient_sound
            } else {
                0
            },
            self.ambient_alone,
            self.ambient_music,
        )
    }
    pub fn ambient_on(&self) -> bool {
        self.ambient_on
    }
    /// The chosen sound, on or off (the settings file keeps it either way).
    pub fn ambient_sound(&self) -> u8 {
        self.ambient_sound
    }
    pub fn set_ambient_on(&mut self, on: bool) {
        self.ambient_on = on;
    }
    /// Unknown ids (a hand-edited file, a newer build's sound) fall back to the default.
    pub fn set_ambient_sound(&mut self, id: u8) {
        self.ambient_sound = if (1..=crate::soundscape::MAX_ID).contains(&id) {
            id
        } else {
            crate::soundscape::DEFAULT_SOUND
        };
    }
    pub fn set_ambient_levels(&mut self, alone: u8, music: u8) {
        self.ambient_alone = alone.min(100);
        self.ambient_music = music.min(100);
    }
    /// Set one level (0 = on its own, 1 = with music); true when it changed.
    fn set_ambient_level(&mut self, row: usize, pct: u8) -> bool {
        let slot = if row == 0 {
            &mut self.ambient_alone
        } else {
            &mut self.ambient_music
        };
        let pct = pct.min(100);
        let changed = *slot != pct;
        *slot = pct;
        changed
    }
    /// The shell reports where the sound is going. Returns true when that changed what is drawn.
    pub fn set_ambient_route(&mut self, r: crate::soundscape::Route) -> bool {
        let changed = self.ambient_route != r;
        self.ambient_route = r;
        changed && self.current() == Screen::Soundscape
    }
    pub fn ambient_route(&self) -> crate::soundscape::Route {
        self.ambient_route
    }

    fn soundscape_view(&self) -> crate::soundscape::View {
        crate::soundscape::View {
            on: self.ambient_on,
            sound: self.ambient_sound,
            alone: self.ambient_alone,
            music: self.ambient_music,
            route: if self.ambient_on {
                self.ambient_route
            } else {
                crate::soundscape::Route::Off
            },
            hook: self.mono_shim,
        }
    }

    fn soundscape_tap(&mut self, x: i32, y: i32) -> Vec<Action> {
        use crate::soundscape as ss;
        if ss::switch_hit(y) {
            self.ambient_sel = ss::SEL_SWITCH;
            self.ambient_on = !self.ambient_on;
            return vec![Action::AmbientChanged];
        }
        if let Some(i) = ss::chip_at(x, y) {
            self.ambient_sel = ss::SEL_FIRST_CHIP + i;
            let id = ss::SOUNDS[i].0;
            // Tapping a sound plays it: a chip is not a preference you set and then go looking for
            // the switch to hear. Tapping the one already playing leaves it playing.
            if self.ambient_on && self.ambient_sound == id {
                return vec![];
            }
            self.ambient_sound = id;
            self.ambient_on = true;
            return vec![Action::AmbientChanged];
        }
        if let Some(row) = ss::level_row_at(y) {
            self.ambient_sel = ss::SEL_ALONE + row;
            if ss::slider_grab(x) && self.set_ambient_level(row, ss::level_at(x)) {
                return vec![Action::AmbientChanged];
            }
        }
        vec![]
    }

    fn soundscape_button(&mut self, b: Button) -> Vec<Action> {
        use crate::soundscape as ss;
        match b {
            Button::Up => {
                self.ambient_sel = self.ambient_sel.saturating_sub(1);
                vec![]
            }
            Button::Down => {
                self.ambient_sel = (self.ambient_sel + 1).min(ss::SEL_COUNT - 1);
                vec![]
            }
            Button::Left | Button::Right if self.ambient_sel >= ss::SEL_ALONE => {
                let row = self.ambient_sel - ss::SEL_ALONE;
                let now = if row == 0 {
                    self.ambient_alone
                } else {
                    self.ambient_music
                };
                let want = if b == Button::Right {
                    now.saturating_add(ss::LEVEL_STEP).min(100)
                } else {
                    now.saturating_sub(ss::LEVEL_STEP)
                };
                if self.set_ambient_level(row, want) {
                    vec![Action::AmbientChanged]
                } else {
                    vec![]
                }
            }
            Button::Select => {
                if self.ambient_sel == ss::SEL_SWITCH {
                    self.ambient_on = !self.ambient_on;
                    return vec![Action::AmbientChanged];
                }
                if (ss::SEL_FIRST_CHIP..ss::SEL_ALONE).contains(&self.ambient_sel) {
                    self.ambient_sound = ss::SOUNDS[self.ambient_sel - ss::SEL_FIRST_CHIP].0;
                    self.ambient_on = true;
                    return vec![Action::AmbientChanged];
                }
                vec![]
            }
            Button::Back => {
                self.pop();
                vec![]
            }
            _ => vec![],
        }
    }

    /// Whether the mono library is running inside SoundServiceFw. Returns true if that changed.
    pub fn set_mono_shim(&mut self, on: bool) -> bool {
        let changed = self.mono_shim != on;
        self.mono_shim = on;
        changed
    }

    /// Which VPT room is selected, 0..=3 — handed straight to Sony's `SetVptMode`. Its own value
    /// for the same reason balance is: `sound_flags` is a u8 of booleans and this is an enum.
    pub fn vpt_mode(&self) -> usize {
        self.snd_vpt_mode
    }
    /// Clamped on the way in, so a hand-edited settings file cannot select a room that does not
    /// exist and get handed to the device as an out-of-range enum.
    pub fn set_vpt_mode(&mut self, m: usize) {
        self.snd_vpt_mode = m.min(VPT_MODES.len() - 1);
    }

    /// The Advanced screen's booleans, packed: bit0 Source Direct, bit1 Clear Phase, bit2 DSEE AI,
    /// bit3 DSEE HX Custom, bit4 Tone Control, bit5 linear headphone amp (read by the shell's
    /// hp_amp_tick, not by the effects chain). Same shape as `sound_flags` and applied by the same
    /// CINDER_ACT_SOUND_CHANGED path.
    ///
    /// Bits 0..=4 are part of the sound PROFILE since R5 (`SoundSetup::adv_bits`): a profile is
    /// every DSP value, and the Advanced screen's own framing — "set these once for a pair of
    /// headphones" — is exactly what a per-output profile is for. Bit 5 is hardware of the jack
    /// alone and stays out of it.
    pub fn adv_flags(&self) -> u8 {
        (self.adv_source_direct as u8)
            | (self.adv_clear_phase as u8) << 1
            | (self.adv_dsee_ai as u8) << 2
            | (self.adv_dsee_custom as u8) << 3
            | (self.adv_tone as u8) << 4
            | (self.adv_hp_linear as u8) << 5
    }
    pub fn set_adv_flags(&mut self, f: u8) {
        self.adv_source_direct = f & 1 != 0;
        self.adv_clear_phase = f & (1 << 1) != 0;
        self.adv_dsee_ai = f & (1 << 2) != 0;
        self.adv_dsee_custom = f & (1 << 3) != 0;
        self.adv_tone = f & (1 << 4) != 0;
        self.adv_hp_linear = f & (1 << 5) != 0;
    }

    /// Tone Control band gains, RAW half-decibels. Clamped on the way in for the same reason the
    /// nudge is: out of range the service zeroes the band rather than clamping it, so a
    /// hand-edited settings file could otherwise silence a band that the UI draws as boosted.
    pub fn tone_bands(&self) -> [i8; crate::tone::BANDS] {
        self.tone_bands
    }
    pub fn set_tone_bands(&mut self, b: [i8; crate::tone::BANDS]) {
        for (dst, src) in self.tone_bands.iter_mut().zip(b.iter()) {
            *dst = (*src).clamp(-crate::tone::BAND_MAX, crate::tone::BAND_MAX);
        }
    }

    /// DAC EQ band gains, RAW half-decibels, in the helper's argument order. Clamped on the way in:
    /// the settings file is on a drive any PC can write, and the helper REFUSES an out-of-range
    /// value outright, so an unchecked one would leave the whole EQ unapplied.
    pub fn dac_eq(&self) -> [i8; crate::dac_eq::BANDS] {
        self.dac_eq
    }
    pub fn set_dac_eq(&mut self, b: [i8; crate::dac_eq::BANDS]) {
        for (dst, src) in self.dac_eq.iter_mut().zip(b.iter()) {
            *dst = crate::dac_eq::clamp(*src);
        }
    }

    /// FM: what the UI currently shows, in kHz.
    pub fn fm_khz(&self) -> i32 {
        self.fm_khz
    }
    /// Report the tuner's ACTUAL frequency back. Used during a live seek so the dial follows the
    /// sweep, and after any tune, because `SetFrequency` rejects out-of-band values and keeps the
    /// previous one — the UI must show what the radio holds, not what it was asked for.
    pub fn fm_report_khz(&mut self, khz: i32) {
        self.fm_khz = khz.clamp(crate::fm::MIN_KHZ, crate::fm::MAX_KHZ);
    }
    pub fn fm_playing(&self) -> bool {
        self.fm_playing
    }
    pub fn fm_set_playing(&mut self, on: bool) {
        self.fm_playing = on;
    }
    pub fn fm_set_antenna(&mut self, present: bool) {
        self.fm_antenna = present;
    }
    /// Raw RSSI from the chip, or <0 for "no meter". See `fm::SIGNAL_FULL` for the scale.
    pub fn fm_set_signal(&mut self, rssi: i32) {
        self.fm_signal = rssi;
    }
    pub fn fm_signal(&self) -> i32 {
        self.fm_signal
    }
    pub fn fm_stereo(&self) -> bool {
        self.fm_stereo
    }
    pub fn fm_hw(&self) -> bool {
        self.fm_hw
    }
    pub fn fm_set_hw(&mut self, hw: bool) {
        self.fm_hw = hw;
    }
    pub fn fm_set_stereo(&mut self, st: bool) {
        self.fm_stereo = st;
    }
    pub fn fm_bt_out(&self) -> bool {
        self.fm_bt_out
    }
    pub fn fm_set_bt_out(&mut self, on: bool) {
        self.fm_bt_out = on;
    }
    pub fn fm_set_rec_available(&mut self, on: bool) {
        self.fm_rec_available = on;
    }
    /// Seconds recorded, or negative when not recording.
    pub fn fm_set_rec_secs(&mut self, secs: i32) {
        self.fm_rec_secs = secs.max(-1);
    }
    pub fn fm_rec_secs(&self) -> i32 {
        self.fm_rec_secs
    }
    pub fn fm_scanning(&self) -> bool {
        self.fm_scanning
    }
    pub fn fm_set_scan_progress(&mut self, pct: u8) {
        self.fm_scan_pct = pct.min(100);
    }
    /// Install the stations a scan found, best first.
    pub fn fm_set_stations(&mut self, khz: &[i32]) {
        self.fm_n_stations = khz.len().min(crate::fm::PRESETS);
        self.fm_stations = [0; crate::fm::PRESETS];
        self.fm_stations[..self.fm_n_stations].copy_from_slice(&khz[..self.fm_n_stations]);
        self.fm_scanning = false;
        self.fm_scan_pct = 0;
    }
    pub fn fm_stations(&self) -> &[i32] {
        &self.fm_stations[..self.fm_n_stations]
    }

    /// DSEE HX Custom mode, 0..=4 — the value for `SetDseeHxCustomMode`.
    pub fn dsee_mode(&self) -> usize {
        self.adv_dsee_mode
    }
    pub fn set_dsee_mode(&mut self, m: usize) {
        self.adv_dsee_mode = m.min(crate::advanced::DSEE_MODES.len() - 1);
    }

    /// Clear Bass gain for the six-band's band 0: 0 (off) to 10, and always 0 where the firmware
    /// cannot run it, so the shell never selects a tone system that is not in the chain.
    pub fn clear_bass(&self) -> usize {
        if self.clear_bass_available {
            self.adv_clear_bass * 2
        } else {
            0
        }
    }
    /// The stored step (an index into `advanced::CLEAR_BASS`), whatever the firmware.
    pub fn clear_bass_step(&self) -> usize {
        self.adv_clear_bass
    }
    pub fn set_clear_bass_step(&mut self, i: usize) {
        self.adv_clear_bass = i.min(crate::advanced::CLEAR_BASS.len() - 1);
    }
    /// The shell's report that the sound library is Walkman One's. Returns whether it changed.
    pub fn set_w1_chain(&mut self, on: bool) -> bool {
        let changed = self.w1_chain != on;
        self.w1_chain = on;
        changed
    }
    /// The shell's report of whether the six-band EQ is in the chain. Returns whether it changed.
    pub fn set_clear_bass_available(&mut self, on: bool) -> bool {
        let changed = self.clear_bass_available != on;
        self.clear_bass_available = on;
        changed
    }

    /// The shell's report that the sound service kept ClearAudio+ off when asked to switch it on:
    /// the firmware has none. The segment goes, and a stored "on" stops claiming the mode in
    /// BOTH setups, so the path strip and the dimming stop describing a mode that is not running.
    /// Returns whether anything changed.
    pub fn set_clearaudio_supported(&mut self, ok: bool) -> bool {
        if self.clearaudio_ok == ok {
            return false;
        }
        self.clearaudio_ok = ok;
        if !ok {
            self.snd_clear = false;
            self.setup_other.clear = false;
        }
        true
    }

    /// Vinylizer character, 0..=3 — the value for `SetVinylizerType`.
    pub fn vinyl_type(&self) -> usize {
        self.adv_vinyl_type
    }
    pub fn set_vinyl_type(&mut self, t: usize) {
        self.adv_vinyl_type = t.min(crate::advanced::VINYL_TYPES.len() - 1);
    }

    /// Which DC Phase filter type is selected, 0..=5 — handed to Sony's `SetDcPhaseFilterType`.
    pub fn dc_type(&self) -> usize {
        self.snd_dc_type
    }
    /// Clamped, so a hand-edited settings file cannot reach the device as an out-of-range enum.
    pub fn set_dc_type(&mut self, t: usize) {
        self.snd_dc_type = t.min(DC_PHASE_TYPES.len() - 1);
    }

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
    pub fn auto_off_min(&self) -> u32 {
        self.auto_off_min
    }
    /// Snap a loaded value to a known preset, so a hand-edited settings file cannot strand the row
    /// on a duration the cycle can never reach again. Same rule as `set_screen_off_s`.
    pub fn set_auto_off_min(&mut self, mins: u32) {
        self.auto_off_idx = AUTO_OFF_PRESETS
            .iter()
            .position(|p| *p == mins)
            .unwrap_or(0);
        self.auto_off_min = AUTO_OFF_PRESETS[self.auto_off_idx];
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

    /// The persisted palette choice, by id — `cinder` for the built-in.
    pub fn palette_id(&self) -> &str {
        &self.palette_wanted
    }

    /// Restore a persisted palette choice. It applies the moment a palette with that id is loaded:
    /// now, if one already is, or whenever the folder is next read.
    pub fn set_palette_wanted(&mut self, id: &str) {
        let id = id.trim().to_ascii_lowercase();
        self.palette_wanted = if id.is_empty() {
            crate::palette::BUILTIN_ID.to_string()
        } else {
            id
        };
        self.resolve_palette();
    }

    /// Install what the palette folder holds (see `palette::load_files`). Returns true when either
    /// the loaded set or the skip reasons changed — the shell's cue to log, once, rather than on
    /// every visit to Settings.
    pub fn set_palettes(&mut self, list: Vec<Palette>, skipped: Vec<String>) -> bool {
        let same_ids = list.len() == self.palettes.len()
            && list.iter().zip(&self.palettes).all(|(a, b)| a.id == b.id);
        let skipped_changed = skipped != self.palette_skipped;
        self.palettes = list;
        self.palette_skipped = skipped;
        self.resolve_palette();
        !same_ids || skipped_changed
    }

    fn resolve_palette(&mut self) {
        self.palette_active = self
            .palettes
            .iter()
            .position(|p| p.id == self.palette_wanted);
        self.palette = self
            .palette_active
            .map_or(CINDER, |i| self.palettes[i].tokens);
    }

    /// The Palette row's value: the palette being drawn, which is Cinder until a chosen palette
    /// has actually loaded.
    pub fn palette_name(&self) -> &str {
        self.palette_active
            .map_or(crate::palette::BUILTIN_NAME, |i| {
                self.palettes[i].name.as_str()
            })
    }

    /// A palette with an accent of its own takes the Accent row out of play.
    pub fn palette_pins_accent(&self) -> bool {
        self.palette.accent.is_some()
    }

    /// True once per visit to Settings: the shell should read the palette folder again.
    pub fn take_palettes_stale(&mut self) -> bool {
        std::mem::take(&mut self.palettes_stale)
    }

    /// The picker's order: the built-in palette first (`None`), then the folder's, by name or by
    /// date added (newest first), per `palette_sort`.
    fn palette_order(&self) -> Vec<Option<usize>> {
        let mut idx: Vec<usize> = (0..self.palettes.len()).collect();
        let by_name = |a: &usize, b: &usize| {
            let (pa, pb) = (&self.palettes[*a], &self.palettes[*b]);
            pa.name
                .to_lowercase()
                .cmp(&pb.name.to_lowercase())
                .then_with(|| pa.id.cmp(&pb.id))
        };
        if self.palette_sort == 1 {
            idx.sort_by(|a, b| {
                self.palettes[*b]
                    .added
                    .cmp(&self.palettes[*a].added)
                    .then_with(|| by_name(a, b))
            });
        } else {
            idx.sort_by(by_name);
        }
        std::iter::once(None)
            .chain(idx.into_iter().map(Some))
            .collect()
    }

    /// Pick a palette by its place in `palette_order`.
    fn pick_palette(&mut self, slot: Option<usize>) {
        self.palette_wanted = match slot {
            Some(i) => self.palettes[i].id.clone(),
            None => crate::palette::BUILTIN_ID.to_string(),
        };
        self.resolve_palette();
    }

    pub fn palette_sort(&self) -> &'static str {
        crate::palette_list::SORT_WORDS[self.palette_sort.min(1)]
    }

    pub fn set_palette_sort(&mut self, word: &str) {
        self.palette_sort = crate::palette_list::SORT_WORDS
            .iter()
            .position(|w| *w == word.trim())
            .unwrap_or(0);
    }

    pub fn bt_codec(&self) -> u8 {
        self.bt_codec
    }
    pub fn bt_ldac_quality(&self) -> u8 {
        self.bt_ldac_quality
    }
    pub fn set_bt_codecs(&mut self, aac: bool, xq: bool) {
        self.bt_aac = aac;
        self.bt_sbc_xq = xq;
        // Plain SBC is a choice only beside SBC XQ; without the component it is just SBC.
        if !xq && self.bt_codec == crate::bluetooth::SBC_STD {
            self.bt_codec = 3;
        }
    }
    /// The chosen codec's name, as this boot sends it.
    pub fn bt_codec_name(&self) -> &'static str {
        crate::bluetooth::codec_name(self.bt_codec, self.bt_sbc_xq)
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
    /// "Turn off when idle" — the shell switches the radio off after ten minutes of a dark screen
    /// with nothing playing over Bluetooth.
    pub fn bt_idle_off(&self) -> bool {
        self.bt_idle_off
    }
    pub fn set_bt_idle_off(&mut self, on: bool) {
        self.bt_idle_off = on;
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
    /// Is the Bluetooth debug log (the HCI capture) switched on?
    pub fn bt_debug_log(&self) -> bool {
        self.bt_debug_log
    }

    /// The shell stopped the capture itself, at its size limit. It toasts what became of the file.
    pub fn bt_debug_log_stopped(&mut self) {
        self.bt_debug_log = false;
    }

    pub fn set_bt_on(&mut self, on: bool) {
        self.bt_on = on;
        if !on {
            self.clear_bt_connecting(); // nothing connects through a radio that is off
        }
    }

    /// Force the USB-DAC toggle to match reality, without raising a `UsbDacToggle` action.
    ///
    /// The toggle is our *intent*; the gadget's `sys.sony.config` is the *fact*, and they diverge
    /// whenever anything changes USB mode outside Cinder. The shell reconciles at startup by
    /// reading the property and calling this. It deliberately does NOT emit an action: the gadget
    /// is already in this state, so re-applying it would flip USB mode for real.
    pub fn set_usb_dac(&mut self, on: bool) {
        self.usb_dac_on = on;
        if !on {
            self.usb_dac_fmt = None;
        }
        self.route_changed();
    }

    /// A tap on the Media servers page.
    fn tap_media(&mut self, y: i32) -> Vec<Action> {
        if crate::media_server::hit_strip(y) {
            if !self.media_playing.is_empty() {
                self.media_playing.clear();
                return vec![Action::RadioStop];
            }
            // On the server list, the strip searches again.
            if self.media_levels.len() <= 1 && !self.media_busy {
                self.media_busy = true;
                self.media_note.clear();
                return vec![Action::MediaSearch];
            }
            return vec![];
        }
        if self.media_busy {
            return vec![];
        }
        let Some(l) = self.media_levels.last() else {
            return vec![];
        };
        let Some(i) = crate::media_server::row_at(y, l.scroll, l.entries.len()) else {
            return vec![];
        };
        if l.searchable && i == 0 {
            self.open_keyboard(KbPurpose::MediaSearch);
            return vec![];
        }
        if l.entries[i].unplayable {
            self.notify("Not a format the player plays");
            return vec![];
        }
        self.media_busy = l.entries[i].folder;
        self.media_note.clear();
        // The shell's rows do not have the search row.
        let index = i - usize::from(l.searchable);
        vec![Action::MediaOpen {
            depth: self.media_levels.len() - 1,
            index,
        }]
    }

    /// The servers the shell found (the first level).
    pub fn media_set_servers(&mut self, servers: Vec<crate::media_server::Entry>) {
        self.media_levels = vec![crate::media_server::Level {
            entries: servers,
            ..Default::default()
        }];
        self.media_busy = false;
    }

    /// A server's or folder's rows, opened from the level `depth - 1` deep. Dropped when Back has
    /// left that level meanwhile. A folder (`searchable`) gets "Search this server" first; a
    /// search's results do not.
    pub fn media_push(
        &mut self,
        depth: usize,
        title: &str,
        mut entries: Vec<crate::media_server::Entry>,
        searchable: bool,
    ) {
        if self.media_levels.len() != depth {
            return;
        }
        if searchable {
            entries.insert(
                0,
                crate::media_server::Entry {
                    title: "Search this server".into(),
                    sub: "Title, artist or album".into(),
                    folder: true,
                    unplayable: false,
                },
            );
        }
        self.media_levels.push(crate::media_server::Level {
            title: title.to_string(),
            entries,
            scroll: 0,
            searchable,
        });
        self.media_busy = false;
    }

    /// What Search this server looks for (the shell reads it for `MediaFind`).
    pub fn media_query(&self) -> &str {
        &self.media_query
    }

    /// A search or a folder failed, or a song stopped with a reason: say so in the strip.
    pub fn media_fail(&mut self, why: &str) {
        self.media_note = why.to_string();
        self.media_busy = false;
    }

    /// The song playing from a server ("" = none), for the strip.
    pub fn media_set_playing(&mut self, what: &str) {
        self.media_playing = what.to_string();
    }

    /// The Skins screen's list, from the shell: the names on the drive and the one in use.
    pub fn set_skin_list(&mut self, names: Vec<String>, current: &str) {
        self.skin_names = names;
        self.skin_cur = current.to_string();
    }

    /// The headphone EQ profile playing, by name ("" = off), for the Advanced row.
    pub fn set_hp_eq(&mut self, name: &str) {
        self.hp_eq = name.to_string();
    }

    /// The name at `i` of the Skins list, for the shell to load.
    pub fn skin_name(&self, i: usize) -> Option<&str> {
        self.skin_names.get(i).map(String::as_str)
    }

    /// llusbdac as the shell sees it: the module is installed, and whether it is loaded. Loaded means
    /// low latency is what is running, so the switch shows it; the shell unloads it when DAC mode
    /// ends, so the switch is free again outside DAC mode.
    pub fn set_usb_dac_low_latency(&mut self, avail: bool, loaded: bool) {
        self.usb_dac_ll_avail = avail;
        if loaded {
            self.usb_dac_ll = true;
        }
    }

    /// Should engaging USB-DAC load llusbdac? Read by the shell on `UsbDacToggle(true)`.
    pub fn usb_dac_low_latency(&self) -> bool {
        self.usb_dac_ll_avail && self.usb_dac_ll
    }

    /// The host's stream format, straight from Sony's `stream_info_t` via the engine. A rate of 0
    /// means "not streaming" and clears the panel back to its generic line, which is what should
    /// happen when the PC stops.
    pub fn set_usb_dac_format(&mut self, rate: u32, bits: u32, chans: u32) {
        self.usb_dac_fmt = if rate == 0 {
            None
        } else {
            Some((rate, bits, chans))
        };
    }

    /// llusbdac's `/proc/llusbdac` line, as read by the shell; an empty or foreign line clears it.
    /// True when the numbers changed (the panel needs a repaint).
    pub fn set_usb_dac_ll_stats(&mut self, line: &str) -> bool {
        let new = crate::usbdac::LlStats::parse(line);
        let changed = new != self.usb_dac_ll_stats;
        self.usb_dac_ll_stats = new;
        changed
    }

    /// The raw negotiated-codec word from `GetSoundStatus`. 0 = not known.
    pub fn set_bt_negotiated_codec(&mut self, raw: u32) {
        self.bt_codec_negotiated = raw;
    }

    /// The negotiated codec's display name, or `None` while the value is unknown or unmapped.
    ///
    /// The table used to be deliberately EMPTY: `GetSoundStatus` had only ever been observed with
    /// the radio OFF, where all four fields read 0, so every entry would have been a guess. It was
    /// filled in on 2026-08-17 from a live link — see `bluetooth::link_codec_name`, which owns the
    /// single mapping so this screen and the Bluetooth screen cannot disagree about what is
    /// playing. Anything still unmapped renders as raw hex rather than a guess.
    pub fn negotiated_codec_name(&self) -> Option<&'static str> {
        crate::bluetooth::link_codec_name(self.bt_link_codec_raw()?, self.bt_sbc_xq)
    }

    /// The negotiated `BtSoundCodec` byte for the CURRENT link, or `None` if nothing is connected
    /// or the service has not reported one. Gated on the peer as well as the value, so a codec from
    /// an old link can never be shown against a new device.
    pub fn bt_link_codec_raw(&self) -> Option<u8> {
        if self.bt_connected.is_none() || self.bt_codec_negotiated == 0 {
            return None;
        }
        u8::try_from(self.bt_codec_negotiated).ok()
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
        // The Winamp skin's display has its own switch: Winamp's spectrum, oscilloscope, off.
        if self.winamp_view() {
            return self.wa_vis < 2;
        }
        self.np_page != 0 || self.viz_on()
    }

    /// Is a visualiser on the glass right now? Now Playing with one showing, or the Visualiser
    /// settings page, whose preview is the whole point of that page — until 2026-10-04 it was fed
    /// nothing there, so the preview never moved and the sample styles showed NO SIGNAL.
    pub fn viz_visible(&self) -> bool {
        (self.wants_spectrum() && self.is_now_playing()) || self.current() == Screen::VizSet
    }
    pub fn set_viz_kind(&mut self, k: u8) {
        self.viz_kind = k % crate::viz::COUNT;
    }

    /// The signal settings as the spectrum code wants them. Built fresh per frame rather than
    /// cached: it is seven field reads and two table lookups, and a cache here would be a second
    /// copy of the settings that has to be invalidated by every row that edits them.
    pub fn viz_cfg(&self) -> crate::vizcfg::VizCfg {
        let (attack_ms, decay_ms, _) = crate::vizcfg::response_from_index(self.viz_response);
        crate::vizcfg::VizCfg {
            scale: crate::vizcfg::Scale::from_index(self.viz_scale),
            range_db: crate::vizcfg::range_from_index(self.viz_range),
            attack_ms,
            decay_ms,
            // One hold time, not a setting: the choice a user actually has is markers or no
            // markers, and 700 ms is long enough to read a transient without the marker becoming a
            // second, slower bar.
            peak_hold_ms: if self.viz_peak_hold { 700.0 } else { 0.0 },
            peak_fall_per_s: 0.9,
            interp: crate::vizcfg::Interp::from_index(self.viz_interp),
            bands: crate::vizcfg::bands_from_index(self.viz_bands),
        }
    }

    /// Step the selected Visualiser row to its next value.
    ///
    /// Every row cycles, including the two that used to live on the Settings list, so the screen
    /// has exactly one interaction and the preview above it answers what the value did. Nothing
    /// here returns an Action: the renderer reads the settings straight out of the app on the next
    /// frame, and the shell picks up the two analyzer-side ones (window, rate) from
    /// `viz_analyzer_params` in its housekeeping tick rather than being interrupted mid-list.
    fn vizset_cycle_row(&mut self) -> Vec<Action> {
        match self.viz_sel {
            crate::vizset::ROW_STYLE => self.viz_kind = (self.viz_kind + 1) % crate::viz::COUNT,
            crate::vizset::ROW_COVER => {
                self.viz_size = (self.viz_size + 1) % crate::viz::SIZE_COUNT
            }
            crate::vizset::ROW_SCALE => {
                self.viz_scale = (self.viz_scale + 1) % crate::vizcfg::Scale::COUNT
            }
            crate::vizset::ROW_RANGE => {
                self.viz_range = (self.viz_range + 1) % crate::vizcfg::RANGE_COUNT
            }
            crate::vizset::ROW_RESPONSE => {
                self.viz_response = (self.viz_response + 1) % crate::vizcfg::RESPONSE_COUNT
            }
            crate::vizset::ROW_CURVE => {
                self.viz_interp = (self.viz_interp + 1) % crate::vizcfg::Interp::COUNT
            }
            crate::vizset::ROW_PEAKS => self.viz_peak_hold = !self.viz_peak_hold,
            crate::vizset::ROW_WINDOW => {
                self.viz_window = (self.viz_window + 1) % crate::vizcfg::WINDOW_COUNT
            }
            crate::vizset::ROW_RATE => {
                self.viz_rate = (self.viz_rate + 1) % crate::vizcfg::RATE_COUNT
            }
            crate::vizset::ROW_BANDS => {
                self.viz_bands = (self.viz_bands + 1) % crate::vizcfg::BANDS_COUNT
            }
            _ => {}
        }
        vec![]
    }

    /// Analyzer arguments for the shell: (emit rate in Hz, detector window in ms — 0 = leave the
    /// service default). The shell converts the window to samples, because the sample rate is a
    /// property of the stream and only it knows one.
    pub fn viz_analyzer_params(&self) -> (u8, u16) {
        (
            crate::vizcfg::rate_from_index(self.viz_rate),
            crate::vizcfg::window_ms_from_index(self.viz_window),
        )
    }

    // Raw index accessors — the settings page renders them and the settings FILE persists them.
    pub fn viz_scale_idx(&self) -> u8 {
        self.viz_scale
    }
    pub fn viz_range_idx(&self) -> u8 {
        self.viz_range
    }
    pub fn viz_response_idx(&self) -> u8 {
        self.viz_response
    }
    pub fn viz_interp_idx(&self) -> u8 {
        self.viz_interp
    }
    pub fn viz_window_idx(&self) -> u8 {
        self.viz_window
    }
    pub fn viz_rate_idx(&self) -> u8 {
        self.viz_rate
    }
    pub fn viz_bands_idx(&self) -> u8 {
        self.viz_bands
    }
    pub fn viz_peak_hold(&self) -> bool {
        self.viz_peak_hold
    }
    pub fn set_viz_scale(&mut self, i: u8) {
        self.viz_scale = i % crate::vizcfg::Scale::COUNT;
    }
    pub fn set_viz_range(&mut self, i: u8) {
        self.viz_range = i % crate::vizcfg::RANGE_COUNT;
    }
    pub fn set_viz_response(&mut self, i: u8) {
        self.viz_response = i % crate::vizcfg::RESPONSE_COUNT;
    }
    pub fn set_viz_interp(&mut self, i: u8) {
        self.viz_interp = i % crate::vizcfg::Interp::COUNT;
    }
    pub fn set_viz_window(&mut self, i: u8) {
        self.viz_window = i % crate::vizcfg::WINDOW_COUNT;
    }
    pub fn set_viz_rate(&mut self, i: u8) {
        self.viz_rate = i % crate::vizcfg::RATE_COUNT;
    }
    pub fn set_viz_bands(&mut self, i: u8) {
        self.viz_bands = i % crate::vizcfg::BANDS_COUNT;
    }
    pub fn set_viz_peak_hold(&mut self, on: bool) {
        self.viz_peak_hold = on;
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
        Tab::Songs => crate::i18n::text("Songs"),
        Tab::Albums => crate::i18n::text("Albums"),
        Tab::Artists => crate::i18n::text("Artists"),
        Tab::Playlists => crate::i18n::text("Playlists"),
    }
}
/// Display title for a Screen, taken from the Menu table (falls back to the app name).
/// What is making the sound when it is not the library (`cinder_set_live_source`, cinder.h's
/// `CINDER_LIVE_*`). 0 = the library.
pub use crate::live;

fn screen_title(s: Screen) -> &'static str {
    match s {
        // The two places that are not Menu rows (see `MENU`), named as the Menu used to name them.
        Screen::NowPlaying => crate::i18n::text("Now Playing"),
        Screen::UpNext => crate::i18n::text("Up Next"),
        Screen::Zappin => crate::i18n::text("Preview"),
        Screen::Display => crate::i18n::text("Display"),
        Screen::Palette => crate::i18n::text("Palette"),
        Screen::Language => crate::i18n::text("Language"),
        Screen::Skins => crate::i18n::text("Skins"),
        Screen::MediaServers => crate::i18n::text("Media servers"),
        Screen::Eq => crate::i18n::text("Equalizer"),
        Screen::NetRadio => crate::i18n::text("Internet radio"),
        s => MENU
            .iter()
            .find(|m| m.0 == s)
            .map(|m| m.1)
            .unwrap_or("Cinder"),
    }
}

#[cfg(test)]
mod language_tests;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod shelf_swipe_tests;

#[cfg(test)]
mod palette_tests;

/// Redesign R4: ratings, play counts, saved views and the playlist editor, driven through `tap`
/// the way a finger drives them.
#[cfg(test)]
mod r4_tests;
