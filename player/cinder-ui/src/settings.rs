//! Settings — interactive. Up/Down move the cursor; Select acts on the focused row.
//!
//! PAGES (owner, 2026-11-03: "rework settings to make things easier to find"). The home page holds
//! what is changed in an ordinary week (Display, Big view, Brightness, Sleep timer) and one row per
//! topic — Playback, Now Playing, Library, Buttons & gestures, Power, Device & storage, System —
//! each opening a page of its rows, its second line naming what is inside. The home page fits the
//! glass. It replaced a More settings row that unfolded twenty rows in place. No row changed what
//! it does; every row acts except Firmware and Model, which are information.
//!
//! Drawn with the redesign kit (`kit.rs`): 64 px rows, 34 px section labels. Theme, palette,
//! accent, text size and the visualiser moved to Settings ▸ Display (`display.rs`, handoff 5k) —
//! the things you change weekly no longer share a column with Restart and Reset.

use crate::kit::{self, Row, Trail};
use crate::text::FontSet;
use crate::theme::Theme;
use crate::Canvas;

/// Number of real rows. The home page's topic links are row ids after them ([`group_row`]).
pub const ROWS: usize = 38;

/// The topic pages: title, the home row's second line (what is inside), and the rows, in order.
/// Every row is on exactly one page: here, or [`HOME_ROWS`].
pub const GROUPS: [(&str, &str, &[usize]); 7] = [
    (
        "Playback",
        "Shuffle · volume limit · DSD",
        &[ROW_SHUFFLE, ROW_VOLUME_LIMIT, ROW_DSD_NATIVE],
    ),
    (
        "Now Playing",
        "Swipes · back arrow · track number",
        &[
            ROW_SWIPE_UP,
            ROW_SWIPE_DOWN,
            ROW_NP_BUTTON,
            ROW_NP_BACK,
            ROW_NP_COUNT,
            ROW_NP_LYRICS,
            ROW_NP_TINT,
        ],
    ),
    (
        "Library",
        "Folders tab · \"The\" · database",
        &[ROW_FOLDERS_TAB, ROW_IGNORE_THE, ROW_DELETE, ROW_DATABASE],
    ),
    (
        "Buttons & gestures",
        "Pull-down panel · Hold · Menu icons",
        &[ROW_QUICK, ROW_HOLD_BUTTONS, ROW_MENU_ICONS],
    ),
    (
        "Power",
        "Screen off · auto off · restart · power off",
        &[
            ROW_SCREEN_OFF,
            ROW_AUTO_OFF,
            ROW_BT_IDLE_OFF,
            ROW_RESTART,
            ROW_POWER_OFF,
        ],
    ),
    (
        "Device & storage",
        "Battery · storage · USB · clock",
        &[ROW_BATTERY, ROW_STORAGE, ROW_USB_MODE, ROW_WIFI, ROW_CLOCK],
    ),
    (
        "System",
        "Language · boot to stock · reset · version",
        &[
            ROW_LANGUAGE,
            ROW_BOOT_STOCK,
            ROW_RESET,
            ROW_FIRMWARE,
            ROW_MODEL,
        ],
    ),
];
/// The home page's own rows, the ones changed in an ordinary week; the topic links follow.
pub const HOME_ROWS: [usize; 6] = [
    ROW_DISPLAY,
    ROW_BIG,
    ROW_BIG_BAR,
    ROW_BIG_ICONS,
    ROW_BRIGHTNESS,
    ROW_SLEEP,
];

/// The home row that opens topic page `g`.
pub const fn group_row(g: usize) -> usize {
    ROWS + g
}
/// The topic page row `r` opens, if it is a topic link.
pub fn group_of_link(r: usize) -> Option<usize> {
    r.checked_sub(ROWS).filter(|g| *g < GROUPS.len())
}
/// The page row `r` is on: `None` for home (its own rows and the topic links).
pub fn page_of(r: usize) -> Option<usize> {
    if HOME_ROWS.contains(&r) || group_of_link(r).is_some() {
        return None;
    }
    GROUPS.iter().position(|(_, _, rows)| rows.contains(&r))
}
/// The rows on `page` as drawn, top to bottom (`lang`: the Language row is shown).
pub fn page_rows(page: Option<usize>, lang: bool) -> Vec<usize> {
    match page {
        None => HOME_ROWS
            .iter()
            .copied()
            .filter(|r| shown(*r, lang))
            .chain((0..GROUPS.len()).map(group_row))
            .collect(),
        Some(g) => GROUPS[g.min(GROUPS.len() - 1)]
            .2
            .iter()
            .copied()
            .filter(|r| shown(*r, lang))
            .collect(),
    }
}
/// The page's header title.
pub fn page_title(page: Option<usize>) -> &'static str {
    crate::i18n::text(page.map_or("Settings", |g| GROUPS[g.min(GROUPS.len() - 1)].0))
}
/// Display ▸ — palette, accent, night, the volume readout, text size and the visualiser.
pub const ROW_DISPLAY: usize = 0;
/// Big view: bigger text and easier-to-press controls, in one switch (`scale::set_large`). In the
/// everyday list, right under Display, because the people who need it are the ones who find small
/// rows hard to reach — it must not sit behind More settings.
pub const ROW_BIG: usize = 1;
/// Big view's two extras (#51), listed under it and shown only while it is on: a bigger top bar
/// (clock, battery, ☰, bookmark) and bigger icons everywhere else. `scale::set_big_bar`/`icons`.
pub const ROW_BIG_BAR: usize = 2;
pub const ROW_BIG_ICONS: usize = 3;

/// Is row `r` on the screen? The Language row needs a second language to switch to, and Big view's
/// two extras need Big view.
pub fn shown(r: usize, lang: bool) -> bool {
    match r {
        ROW_LANGUAGE => lang,
        ROW_BIG_BAR | ROW_BIG_ICONS => crate::scale::large(),
        // Only with the Wi-Fi install option: the player's own firmware has no Wi-Fi at all.
        ROW_WIFI => crate::wifi::installed(),
        _ => true,
    }
}
pub const ROW_BRIGHTNESS: usize = 4;
pub const ROW_SCREEN_OFF: usize = 5;
/// The pull-down panel (`quick.rs`): a swipe down from the status bar opens brightness, Bluetooth,
/// night and the sleep timer. Asked for on the r/walkman thread; OFF by default, because the owner
/// uses the Shelf and a new gesture must never arrive unasked (`docs/PLAN_community_2026-09-23.md`
/// B1). With it off there is no gesture and nothing else on any screen changes.
pub const ROW_QUICK: usize = 20;
/// Swipe up / swipe down on Now Playing (#51): each opens the screen picked here, or nothing. OFF
/// by default, like every community-asked gesture. They are the middle of the screen only: a swipe
/// up from the bottom edge stays the Shelf's and a pull from the status bar the pull-down panel's.
pub const ROW_SWIPE_UP: usize = 15;
pub const ROW_SWIPE_DOWN: usize = 16;
/// The third button on Now Playing's toolbar (#51): Bluetooth, as always, or Sound, Equalizer or the
/// Menu. `now_playing::NpButton`.
pub const ROW_NP_BUTTON: usize = 17;
/// A back arrow at the top left of Now Playing (#51, plaquette): the same as the left-edge swipe,
/// for anyone who does not know the swipe or finds it hard. OFF by default.
pub const ROW_NP_BACK: usize = 18;
/// The track's place in what is playing ("3 / 12") on Now Playing (#51, plaquette). OFF by default.
pub const ROW_NP_COUNT: usize = 19;
/// Now Playing's background takes the cover's colour (`Theme::tinted`). OFF by default.
pub const ROW_NP_TINT: usize = 33;
/// The Lyrics chip at the top left of Now Playing, for a song with lyrics. OFF by default (owner,
/// 2026-11-14); the lyrics stay one tap away on Track information either way.
pub const ROW_NP_LYRICS: usize = 35;
/// Hold locks the player's own buttons too, not only the screen (#51, plaquette). Power still
/// toggles the screen and headphone buttons still work. OFF by default: under Hold the side
/// buttons have always controlled playback.
pub const ROW_HOLD_BUTTONS: usize = 21;
/// Volume limit — a safe-listening cap on the 3.5 mm level.
///
/// The CAP is Sony's, not ours: `VolumeService` reports an AVLS threshold per output device
/// (63/120 on this unit, `adapt=1`), and it is a real limiter — measured on device 2026-09-07,
/// with it enabled a request for 91 came back 63. What Cinder does NOT do is switch Sony's flag
/// on, because Sony enforces inside `VolumeAdlerOut::SetVolume` and Cinder writes the mixer
/// directly; the flag would be a control that accepts a write and changes nothing, which is
/// exactly what "High gain output" turned out to be (see sound.rs). So the shell reads Sony's
/// number and clamps in its own `apply_volume`.
pub const ROW_VOLUME_LIMIT: usize = 24;
pub const ROW_SLEEP: usize = 6;
/// What the shuffle button deals: SONGS (every track at random, as always), ALBUMS (whole albums
/// at random, each in its own order) or ARTISTS. `shuffle.rs`. SONGS by default, so the button
/// does what it has always done until the owner picks otherwise.
pub const ROW_SHUFFLE: usize = 7;
/// DSD files go to the headphone jack as 1-bit, without Sony's conversion to PCM. OFF by default:
/// Sony's A50 build always converts (analysis/RE_walkmanone_tunings.md section 10), so this is the
/// player doing something its own firmware never did. The shell sends Sony's two settings
/// (`builtin_dsd_process_mode`, `se_hp_dsd_native_enabled`; both are needed, measured) when it
/// starts and when this flips.
pub const ROW_DSD_NATIVE: usize = 34;
/// Ignore "The" in artists — sort "The Beatles" among the B's in the Artists tab, the Albums tab's
/// artist groups and Songs by artist (and file it under B on the rail). OFF by default: artists
/// sort as written. Titles and album names keep their "The" either way. See `collate`.
pub const ROW_IGNORE_THE: usize = 12;
pub const ROW_DATABASE: usize = 13;
/// Folders tab: a FOLDERS link at the end of the Library's tab strip that opens the folder browser
/// (#45, asked for on GitHub). OFF by default, like every community-asked UI change: with it off the
/// strip is exactly the four tabs it always was.
pub const ROW_FOLDERS_TAB: usize = 14;
/// Delete music: a DELETE action on Track information (the playing song) and on the album page
/// (the whole album), each behind a confirmation. Asked for by the owner once the player could
/// fetch music by itself (Wi-Fi drive, phone uploads). OFF by default: with it off no screen can
/// remove a file, which is how the player has always been.
pub const ROW_DELETE: usize = 31;
/// Menu icons: an icon beside each Menu row (discussion #53, "easier to navigate than text only").
/// OFF by default, like every community-asked change to a screen.
pub const ROW_MENU_ICONS: usize = 32;
/// Auto power-off: shut the device down after N minutes of no input AND nothing playing. Sony has
/// this (sid_4118 AutoShutdownSetting) and Cinder did not, so a paused device with the screen dark
/// ran until the battery was flat. 30 MIN by default since 2026-10-04 (it was OFF): the owner's rule
/// is that no state may drain a player nobody is using. OFF is one tap away.
pub const ROW_AUTO_OFF: usize = 22;
/// Bluetooth auto off: switch the radio off after ten minutes of a dark screen with nothing
/// playing over it. The radio (and a link carrying nothing) costs power for as long as it is up,
/// and nothing but the user ever switched it off. ON by default since 2026-10-04, like Auto power
/// off. It does not come back by itself.
pub const ROW_BT_IDLE_OFF: usize = 23;
pub const ROW_STORAGE: usize = 25;
pub const ROW_BATTERY: usize = 9;
/// Date & time. Sony has this and Cinder did not — the status-bar clock was read-only, so a
/// drifting RTC or a flat battery left no way back to a correct time short of booting stock. The
/// row drills into `clockset`; the shell writes both clocks through the setuid `cinder-clock`
/// helper, because nothing in vendor/sony/lib exposes a clock setter and cinder-home is uid 100.
pub const ROW_CLOCK: usize = 8;
pub const ROW_USB_MODE: usize = 10; // tapping enters USB mass-storage (file transfer to a PC)
/// Boot to stock: arms a ONE-SHOT return to Sony's player, then restarts. Two taps (the row asks
/// for confirmation first) because it reboots the device.
pub const ROW_BOOT_STOCK: usize = 26;
/// Restart and Power off. Both go through the confirmation modal — they take the device away
/// mid-song, and the two-tap row used by Boot to stock is too easy to arm by accident for that.
pub const ROW_RESTART: usize = 27;
pub const ROW_POWER_OFF: usize = 11;
/// Reset every preference to its default. Sony has this (sid_4106 "Reset Settings") and it is the
/// only way out of a settings state you cannot see your way back from — a wrong UI scale, a dark
/// theme at brightness 1, an EQ you have lost track of. Behind the confirmation modal, because it
/// throws away work; it does NOT touch the library, what is playing, or the shelf pins.
pub const ROW_RESET: usize = 28;
/// ABOUT — static info rows, but they still take the cursor, so they need names like the rest.
pub const ROW_LANGUAGE: usize = 29;
pub const ROW_FIRMWARE: usize = 36;
pub const ROW_MODEL: usize = 37;
/// Wi-Fi (`wifi.rs`): shown only when the Wi-Fi install option is on the player.
pub const ROW_WIFI: usize = 30;

/// What a swipe on Now Playing opens (`ROW_SWIPE_UP`, `ROW_SWIPE_DOWN`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SwipeTo {
    #[default]
    Off,
    Menu,
    Sound,
    Eq,
    UpNext,
    Bluetooth,
    Library,
    Folders,
}

impl SwipeTo {
    /// The order the row steps through.
    pub const ALL: [SwipeTo; 8] = [
        SwipeTo::Off,
        SwipeTo::Menu,
        SwipeTo::Sound,
        SwipeTo::Eq,
        SwipeTo::UpNext,
        SwipeTo::Bluetooth,
        SwipeTo::Library,
        SwipeTo::Folders,
    ];

    /// The settings-file word. A word, so a new target cannot shift what an existing file means.
    pub fn token(self) -> &'static str {
        match self {
            SwipeTo::Off => "off",
            SwipeTo::Menu => "menu",
            SwipeTo::Sound => "sound",
            SwipeTo::Eq => "eq",
            SwipeTo::UpNext => "upnext",
            SwipeTo::Bluetooth => "bluetooth",
            SwipeTo::Library => "library",
            SwipeTo::Folders => "folders",
        }
    }

    pub fn from_token(s: &str) -> SwipeTo {
        crate::from_token(&Self::ALL, Self::token, s)
    }

    /// The Settings row's value.
    pub fn label(self) -> &'static str {
        match self {
            SwipeTo::Off => crate::i18n::text("OFF"),
            SwipeTo::Menu => crate::i18n::text("MENU"),
            SwipeTo::Sound => crate::i18n::text("SOUND"),
            SwipeTo::Eq => crate::i18n::text("EQUALIZER"),
            SwipeTo::UpNext => crate::i18n::text("UP NEXT"),
            SwipeTo::Bluetooth => crate::i18n::text("BLUETOOTH"),
            SwipeTo::Library => crate::i18n::text("LIBRARY"),
            SwipeTo::Folders => crate::i18n::text("FOLDERS"),
        }
    }

    pub fn next(self) -> SwipeTo {
        crate::next_in(&Self::ALL, self)
    }
}

#[allow(non_snake_case)]
pub(crate) fn RH() -> i32 {
    kit::ROW_H()
}

/// The Display row's second line: what is behind it (handoff 2c).
pub const DISPLAY_SUB: &str = "Palette · accent · volume display";
/// The Shuffle row's second line: which control the setting is about.
pub const SHUFFLE_SUB: &str = "What the shuffle button deals";
/// The pull-down panel row's second line: where the gesture starts and what it opens.
pub const QUICK_SUB: &str = "Swipe down from the top of the screen";
/// The Big view row's second line: what it changes.
pub const BIG_SUB: &str = "Bigger text, easier-to-press controls";
/// The Bluetooth auto off row's second line: when it acts.
pub const BT_IDLE_OFF_SUB: &str = "Sleeps after 10 min idle, wakes on use";

/// The released version, in ONE place. `tools/release.sh` rewrites this line when it bumps the
/// tag, the same way it rewrites `installer/Cargo.toml` — so what the player shows on its own
/// screen and what the release is called cannot drift apart. It is a macro rather than a `const`
/// because `concat!` takes literals only.
// One line, and rustfmt must leave it so: `tools/release.sh` finds and rewrites it by its shape.
#[macro_export]
#[rustfmt::skip]
macro_rules! cinder_version { () => { "0.4.2-rc1" } }

/// Firmware/build label shown on the Settings "Firmware" row. The `dev` feature (development
/// channel, built from the same tree) makes the two builds visually distinguishable on-device.
///
/// It carries the VERSION as of 0.3.12. Until then the stable label read "CINDER 1.0 · RUST" —
/// a number that was never released and never changed, so a player could not say which build was
/// on it, and neither could a bug report.
#[cfg(feature = "dev")]
pub const FIRMWARE_LABEL: &str = concat!("CINDER ", cinder_version!(), " · DEV");
#[cfg(not(feature = "dev"))]
pub const FIRMWARE_LABEL: &str = concat!("CINDER ", cinder_version!());

/// Current settings values to display.
pub struct SettingsView<'a> {
    pub usb_dac: bool,
    pub battery_care: bool,
    /// One-line summary for the Device row, e.g. "99% · 34.4 °C". Formatted by `nav` with the same
    /// helpers the Device screen uses, so the row and the screen can never disagree about a number.
    pub device: &'a str,
    pub storage: &'a str,
    /// Settings ▸ Database value: "N tracks", "Empty", or "Rescanning…" while a scan is out.
    pub database: &'a str,
    pub sleep: &'a str,
    /// Brightness label, e.g. "3 / 5" (nav formats it from its 1..5 level).
    pub brightness: &'a str,
    /// Idle screen-off label, e.g. "OFF" / "30 SEC" / "2 MIN".
    pub screen_off: &'a str,
    /// Auto power-off label, e.g. "OFF" / "30 MIN".
    pub auto_off: &'a str,
    /// Bluetooth auto off: the switch.
    pub bt_idle_off: bool,
    /// Boot-to-stock row value: normally "SONY", or the confirm prompt once armed.
    pub boot_stock: &'a str,
    /// The live clock, shown as the Date & time row's value — so the row is also where you notice
    /// the time is wrong. Formatted by the caller (nav) from the same string the status bar uses.
    pub clock: &'a str,
    /// Volume limit on/off. A toggle, not a value: the cap itself is Sony's and is read live by
    /// the shell, so there is no number here for the user to pick.
    pub volume_limit: bool,
    /// Big view on/off (`ROW_BIG`).
    pub big_view: bool,
    /// Big view's extras, as set (`ROW_BIG_BAR`, `ROW_BIG_ICONS`).
    pub big_bar: bool,
    pub big_icons: bool,
    /// Ignore "The" at the start of artist names when sorting (`ROW_IGNORE_THE`).
    pub ignore_the: bool,
    /// The opt-in switches; each one with a row here is drawn as that row's switch.
    pub opts: crate::opt::Opts,
    /// What a swipe up / down on Now Playing opens (`ROW_SWIPE_UP`, `ROW_SWIPE_DOWN`).
    pub swipe_up: SwipeTo,
    pub swipe_down: SwipeTo,
    /// Now Playing's third toolbar button (`ROW_NP_BUTTON`).
    pub np_button: crate::now_playing::NpButton,
    /// What shuffle deals (`ROW_SHUFFLE`): "SONGS", "ALBUMS" or "ARTISTS".
    pub shuffle_by: &'a str,
    /// The page on screen: `None` for home, else a [`GROUPS`] index.
    pub page: Option<usize>,
    /// The languages this install offers, from the installer. The Language row is shown only when
    /// there is more than one to switch between.
    pub languages: &'a [crate::i18n::Language],
}

/// The Language row's title: the word for "language" in every installed language, in its own
/// script, so whoever is stuck in a language they cannot read still finds the way out.
pub fn language_title(languages: &[crate::i18n::Language]) -> String {
    let mut words: Vec<&str> = languages.iter().map(|l| l.word()).collect();
    words.dedup();
    words.join(" / ")
}

/// Total height of the page, from the top of the screen to the bottom of the last row.
pub fn content_height(page: Option<usize>, lang: bool) -> i32 {
    LIST_TOP + page_rows(page, lang).len() as i32 * RH()
}

/// How far the page can scroll: 0 when it fits (the home page does; Big view's taller rows may not).
pub fn max_scroll_px(page: Option<usize>, lang: bool) -> i32 {
    // The 8 px margin under the last row is for a list that scrolls: one that fits must not scroll
    // 3 px to show a margin.
    let h = content_height(page, lang);
    if h <= crate::canvas::H as i32 {
        0
    } else {
        h + 8 - crate::canvas::H as i32
    }
}

/// Which row is at touch-y `y` on `page`, given the `scroll` offset? Mirrors `render` exactly:
/// the header ends at `LIST_TOP` and every row is `RH()` tall. A row id, a topic link
/// ([`group_row`]) on home, or None.
pub fn row_at(y: i32, scroll: i32, page: Option<usize>, lang: bool) -> Option<usize> {
    let i = (y + scroll - LIST_TOP).div_euclid(RH());
    if y < LIST_TOP || i < 0 {
        return None;
    }
    page_rows(page, lang).get(i as usize).copied()
}

/// Screen-y of the top of the row list (before scrolling). Exposed for tests and for nav's
/// "keep the cursor visible" arithmetic — the layout constant lives here, not in the caller.
pub const LIST_TOP: i32 = 91;

/// Content-space top y of row `r` on its own page (at scroll 0, measured from LIST_TOP).
pub fn row_top_px(r: usize) -> i32 {
    page_rows(page_of(r), true)
        .iter()
        .position(|x| *x == r)
        .map_or(0, |i| i as i32 * RH())
}

/// The trailing value of row `r`.
fn trail<'a>(r: usize, v: &'a SettingsView) -> Trail<'a> {
    if let Some(o) = crate::opt::Opt::of_row(r) {
        return Trail::Switch(v.opts.on(o));
    }
    match r {
        ROW_LANGUAGE => Trail::Open(crate::i18n::current().name()),
        ROW_DISPLAY => Trail::Open(""),
        ROW_BRIGHTNESS => Trail::Value(v.brightness),
        ROW_SCREEN_OFF => Trail::Value(v.screen_off),
        ROW_SWIPE_UP => Trail::Value(v.swipe_up.label()),
        ROW_SWIPE_DOWN => Trail::Value(v.swipe_down.label()),
        ROW_NP_BUTTON => Trail::Value(v.np_button.label()),
        ROW_BIG => Trail::Switch(v.big_view),
        ROW_BIG_BAR => Trail::Switch(v.big_bar),
        ROW_BIG_ICONS => Trail::Switch(v.big_icons),
        // The volume limit is a value row rather than a switch because the interesting half is
        // WHOSE limit it is: "SAFE LEVEL" says a cap is in force without inventing a number the
        // user did not choose, and the number is Sony's per-output AVLS threshold, read live.
        ROW_VOLUME_LIMIT => Trail::Value(if v.volume_limit {
            crate::i18n::text("SAFE LEVEL")
        } else {
            crate::i18n::text("OFF")
        }),
        ROW_SLEEP => Trail::Value(v.sleep),
        ROW_SHUFFLE => Trail::Value(v.shuffle_by),
        ROW_IGNORE_THE => Trail::Value(if v.ignore_the {
            crate::i18n::text("ON")
        } else {
            crate::i18n::text("OFF")
        }),
        // Database: CHEVRON, because tapping does something — it asks Sony's MediaStore to rescan
        // the music tree. The value carries the library size, so the row also answers "did it work".
        ROW_DATABASE => Trail::Open(v.database),
        ROW_AUTO_OFF => Trail::Value(v.auto_off),
        ROW_BT_IDLE_OFF => Trail::Switch(v.bt_idle_off),
        // Storage: chevron into the Storage screen. The value keeps the statvfs reading and the
        // card's half goes on the sub line (`storage_parts`), so the row still answers at a glance.
        ROW_STORAGE => Trail::Open(storage_parts(v.storage).0),
        // Device: chevron into the hardware's vital signs. The value carries the two numbers people
        // open it for, so the row usually answers the question without being opened at all.
        ROW_BATTERY => Trail::Open(v.device),
        // The value is the live clock, so the row doubles as the place you notice it is wrong.
        ROW_CLOCK => Trail::Open(v.clock),
        ROW_WIFI => Trail::Open(crate::wifi::row_value()),
        ROW_USB_MODE => Trail::Open(if v.usb_dac {
            "DAC"
        } else {
            crate::i18n::text("MASS STORAGE")
        }),
        // The value doubles as the confirmation prompt (see nav: first tap arms, second goes).
        ROW_BOOT_STOCK => Trail::Open(v.boot_stock),
        ROW_RESTART | ROW_POWER_OFF => Trail::Open(""),
        r if group_of_link(r).is_some() => Trail::Open(""),
        // Names the SCOPE, since "reset" on a music player could just as easily mean the library.
        ROW_RESET => Trail::Open(crate::i18n::text("PREFERENCES")),
        ROW_FIRMWARE => Trail::Value(FIRMWARE_LABEL),
        _ => Trail::Value("SONY NW-A55"),
    }
}

fn title(r: usize) -> &'static str {
    match r {
        ROW_DISPLAY => crate::i18n::text("Display"),
        ROW_BRIGHTNESS => crate::i18n::text("Brightness"),
        ROW_SCREEN_OFF => crate::i18n::text("Screen-off timer"),
        ROW_QUICK => crate::i18n::text("Pull-down panel"),
        ROW_FOLDERS_TAB => crate::i18n::text("Folders tab"),
        ROW_DELETE => crate::i18n::text("Delete music"),
        ROW_MENU_ICONS => crate::i18n::text("Menu icons"),
        ROW_SWIPE_UP => crate::i18n::text("Swipe up"),
        ROW_SWIPE_DOWN => crate::i18n::text("Swipe down"),
        ROW_NP_BUTTON => crate::i18n::text("Now Playing button"),
        ROW_NP_BACK => crate::i18n::text("Back arrow"),
        ROW_NP_COUNT => crate::i18n::text("Track number"),
        ROW_NP_TINT => crate::i18n::text("Cover colours"),
        ROW_NP_LYRICS => crate::i18n::text("Lyrics button"),
        ROW_HOLD_BUTTONS => crate::i18n::text("Hold locks the buttons"),
        ROW_BIG => crate::i18n::text("Big view"),
        ROW_BIG_BAR => crate::i18n::text("Bigger top bar"),
        ROW_BIG_ICONS => crate::i18n::text("Bigger icons"),
        ROW_VOLUME_LIMIT => crate::i18n::text("Volume limit"),
        ROW_SLEEP => crate::i18n::text("Sleep timer"),
        ROW_SHUFFLE => crate::i18n::text("Shuffle"),
        ROW_DSD_NATIVE => crate::i18n::text("Play DSD natively"),
        ROW_IGNORE_THE => crate::i18n::text("Ignore \"The\" in artists"),
        ROW_DATABASE => crate::i18n::text("Database"),
        ROW_AUTO_OFF => crate::i18n::text("Auto power off"),
        ROW_BT_IDLE_OFF => crate::i18n::text("Bluetooth auto off"),
        ROW_STORAGE => crate::i18n::text("Storage"),
        ROW_BATTERY => crate::i18n::text("Device"),
        ROW_CLOCK => crate::i18n::text("Date & time"),
        ROW_USB_MODE => crate::i18n::text("USB mode"),
        ROW_WIFI => crate::i18n::text("Wi-Fi"),
        ROW_BOOT_STOCK => crate::i18n::text("Boot to stock"),
        ROW_RESTART => crate::i18n::text("Restart"),
        ROW_POWER_OFF => crate::i18n::text("Power off"),
        r if group_of_link(r).is_some() => page_title(group_of_link(r)),
        ROW_RESET => crate::i18n::text("Reset settings"),
        ROW_FIRMWARE => crate::i18n::text("Firmware"),
        _ => crate::i18n::text("Model"),
    }
}

/// The Storage row's value and sub line: internal storage, and the card (`"…, SD 108 / 119 GB"`,
/// as the shell's `report_storage` writes it). On one line the card's numbers ran past the row and
/// were cut to "SD 108…" at 120 % (seen on the player 2026-10-24).
fn storage_parts(s: &str) -> (&str, &str) {
    s.split_once(", ").unwrap_or((s, ""))
}

/// A row's second line.
fn sub_line<'a>(r: usize, v: &'a SettingsView) -> &'a str {
    match r {
        ROW_DISPLAY => crate::i18n::text(DISPLAY_SUB),
        r if group_of_link(r).is_some() => {
            match GROUPS[group_of_link(r).unwrap_or(0)].1 {
                // The page has a Wi-Fi row only with the Wi-Fi option installed; say so when it does.
                "Battery · storage · USB · clock" if crate::wifi::installed() => {
                    crate::i18n::text("Battery · storage · USB · Wi-Fi · clock")
                }
                sub => crate::i18n::text(sub),
            }
        }
        ROW_QUICK => crate::i18n::text(if crate::scale::large() {
            "Swipe down from screen top"
        } else {
            QUICK_SUB
        }),
        ROW_BIG => crate::i18n::text(BIG_SUB),
        ROW_BIG_BAR => crate::i18n::text("Clock, battery and menu"),
        ROW_BIG_ICONS => crate::i18n::text("Arrows and symbols"),
        ROW_SWIPE_UP | ROW_SWIPE_DOWN => crate::i18n::text("Opens the screen you pick"),
        ROW_NP_BUTTON => crate::i18n::text("The third one along the bottom"),
        ROW_NP_BACK => crate::i18n::text("Top left, like the edge swipe"),
        ROW_NP_COUNT => crate::i18n::text("Its place in what's playing"),
        ROW_NP_TINT => crate::i18n::text("Background from the album art"),
        ROW_NP_LYRICS => crate::i18n::text("Top left, when the song has lyrics"),
        ROW_HOLD_BUTTONS => crate::i18n::text("Headphone buttons still work"),
        ROW_DELETE => crate::i18n::text("From Track information and album pages"),
        ROW_MENU_ICONS => crate::i18n::text("In the Menu and the pull-down panel"),
        ROW_SHUFFLE => crate::i18n::text(SHUFFLE_SUB),
        ROW_DSD_NATIVE => crate::i18n::text("To the headphone jack as 1-bit"),
        ROW_BT_IDLE_OFF => crate::i18n::text(BT_IDLE_OFF_SUB),
        ROW_STORAGE => storage_parts(v.storage).1,
        _ => "",
    }
}

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, sel: usize, scroll: i32, v: &SettingsView) {
    c.fill(t.bg);
    let y0 = crate::chrome::header(c, t, f, page_title(v.page), None);
    // A page taller than the panel scrolls (Big view). Rows are drawn shifted up by `scroll`;
    // row_at applies the same shift, so the hit test can't drift from the render.
    //
    // CLIP TO BELOW THE HEADER, or the scrolled rows paint over the title and the back chevron.
    // (The status bar is safe either way: the navigator draws it AFTER every screen.)
    let mut y = y0 - scroll;
    c.set_clip_y(crate::chrome::HEADER_BOTTOM, crate::canvas::H as i32);
    let language_title = language_title(v.languages);
    for r in page_rows(v.page, v.languages.len() >= 2) {
        let row = Row::new(if r == ROW_LANGUAGE {
            &language_title
        } else {
            title(r)
        })
        .sub(sub_line(r, v))
        .trail(trail(r, v))
        .sel(sel == r);
        y = if r == ROW_LANGUAGE {
            crate::text::with_fixed_ui_glyphs(|| kit::row(c, t, f, y, RH(), &row))
        } else {
            kit::row(c, t, f, y, RH(), &row)
        };
    }
    c.clear_clip();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::FontSet;

    fn view<'a>() -> SettingsView<'a> {
        SettingsView {
            volume_limit: false,
            big_view: false,
            big_bar: false,
            big_icons: false,
            opts: Default::default(),
            swipe_up: SwipeTo::Off,
            swipe_down: SwipeTo::Off,
            np_button: crate::now_playing::NpButton::Bluetooth,
            usb_dac: false,
            battery_care: true,
            device: "78% · 34.4 °C",
            database: "3,424 tracks",
            storage: "12.4 / 58 GB",
            sleep: "30 MIN",
            brightness: "4 / 5",
            screen_off: "OFF",
            auto_off: "OFF",
            bt_idle_off: false,
            boot_stock: "SONY",
            clock: "17 Aug · 09:01",
            ignore_the: false,
            shuffle_by: "SONGS",
            page: None,
            languages: &crate::i18n::Language::ALL,
        }
    }

    #[test]
    fn english_language_entry_uses_the_bundled_glyphs_without_device_fallbacks() {
        let _scale = crate::text::scale_guard();
        let _language = crate::i18n::enter(crate::i18n::Language::English);
        let theme = Theme::day();
        let fonts = FontSet::load();
        let mut canvas = Canvas::new();
        let v = SettingsView {
            page: page_of(ROW_LANGUAGE),
            ..view()
        };
        render(&mut canvas, &theme, &fonts, 0, 0, &v);

        let glyph_font =
            crate::text::with_fixed_ui_glyphs(|| crate::text::probe_glyph(&fonts, '语'));
        assert_eq!(
            glyph_font, 8,
            "the fixed bilingual label uses the bundled subset"
        );
        assert_eq!(
            fonts.chain_walks(),
            0,
            "the label must not request system fonts"
        );
        assert!(!fonts.unresolved_chars().contains(&'语'));
        assert!(!fonts.unresolved_chars().contains(&'言'));
        assert!(canvas
            .buf
            .iter()
            .any(|pixel| *pixel != crate::canvas::to_u32(theme.bg)));
    }

    /// The card's usage reads whole: it was one value with internal storage and was cut to
    /// "SD 108…" at 120 % (the player, 2026-10-24). Now it is the row's sub line.
    #[test]
    fn the_cards_storage_is_not_cut_off() {
        let _scale = crate::text::scale_guard();
        crate::text::set_scale_pct(120);
        let _language = crate::i18n::enter(crate::i18n::Language::English);
        let fonts = FontSet::load();
        let mut canvas = Canvas::new();
        canvas.track_text();
        let mut v = view();
        v.storage = "3.4 / 56 GB, SD 108 / 119 GB";
        v.page = page_of(ROW_STORAGE);
        render(&mut canvas, &Theme::day(), &fonts, 0, 0, &v);
        let runs = canvas.text_runs();
        assert!(runs.iter().any(|r| r == "3.4 / 56 GB"), "{runs:?}");
        assert!(runs.iter().any(|r| r == "SD 108 / 119 GB"), "{runs:?}");
    }

    /// Scrolling must never repaint the header. The rows move UP as you scroll, and without a clip
    /// they carry on past HEADER_BOTTOM and cover the "Settings" title and the back chevron — the
    /// header is drawn first, so anything drawn after it simply wins. Reported from the device
    /// 2026-07-28 ("the top bars get covered up when scrolling").
    ///
    /// The check is a pixel diff of the whole header band against an unscrolled frame, so it will
    /// catch any future control that reaches up there, not just the rows that did.
    #[test]
    fn scrolling_never_paints_over_the_header() {
        // Renders text, so it must not run concurrently with a test that moves the UI scale.
        let _scale = crate::text::scale_guard();
        let t = Theme::day();
        let f = FontSet::load();
        let mut base = Canvas::new();
        render(&mut base, &t, &f, 0, 0, &view());

        // render draws at whatever scroll it is given; nav clamps it.
        for scroll in [1, 17, 60, 150, 300] {
            let mut c = Canvas::new();
            render(&mut c, &t, &f, 0, scroll, &view());
            for y in 0..crate::chrome::HEADER_BOTTOM {
                for x in 0..crate::canvas::W {
                    let i = y as usize * crate::canvas::W + x;
                    assert_eq!(
                        c.buf[i], base.buf[i],
                        "scroll {scroll} painted over the header at ({x},{y})"
                    );
                }
            }
        }
    }

    /// …and the clip must be released again, or every later screen in the same frame would inherit
    /// it. The navigator draws the status bar and the Now Playing bar after the screen.
    #[test]
    fn the_clip_is_released_after_rendering() {
        let _scale = crate::text::scale_guard();
        let t = Theme::day();
        let f = FontSet::load();
        let mut c = Canvas::new();
        render(&mut c, &t, &f, 0, 300, &view());
        // A full-screen fill must reach row 0 again; if the clip leaked, the top band stays put.
        c.fill(t.acc);
        assert_eq!(
            c.buf[0],
            crate::canvas::to_u32(t.acc),
            "clip band leaked out of settings::render"
        );
    }

    /// Scrolling still has to actually move the rows — otherwise the test above passes trivially.
    #[test]
    fn scrolling_moves_the_content() {
        let _scale = crate::text::scale_guard();
        let t = Theme::day();
        let f = FontSet::load();
        let mut a = Canvas::new();
        render(&mut a, &t, &f, 0, 0, &view());
        let mut b = Canvas::new();
        render(&mut b, &t, &f, 0, 300, &view());
        let band = (crate::chrome::HEADER_BOTTOM as usize)..(crate::canvas::H);
        let differing = band
            .flat_map(|y| (0..crate::canvas::W).map(move |x| y * crate::canvas::W + x))
            .filter(|&i| a.buf[i] != b.buf[i])
            .count();
        assert!(
            differing > 5000,
            "scrolling barely changed the list ({differing} px)"
        );
    }

    /// One installed language: no Language row, and everything after it moves up into its place,
    /// in the hit test and the drawing alike.
    #[test]
    fn a_single_language_install_has_no_language_row() {
        let system = page_of(ROW_LANGUAGE);
        let firmware = |lang| {
            let at = page_rows(system, lang)
                .iter()
                .position(|r| *r == ROW_FIRMWARE)
                .unwrap() as i32;
            let top = LIST_TOP + at * RH();
            assert_eq!(row_at(top + RH() / 2, 0, system, lang), Some(ROW_FIRMWARE));
            top
        };
        assert_eq!(firmware(false), firmware(true) - RH());
        assert!(!page_rows(system, false).contains(&ROW_LANGUAGE));

        let _scale = crate::text::scale_guard();
        let (t, f) = (Theme::day(), FontSet::load());
        let (mut one, mut both) = (Canvas::new(), Canvas::new());
        let single = [crate::i18n::Language::English];
        let v1 = SettingsView {
            languages: &single,
            page: system,
            ..view()
        };
        let v2 = SettingsView {
            page: system,
            ..view()
        };
        render(&mut one, &t, &f, 0, 0, &v1);
        render(&mut both, &t, &f, 0, 0, &v2);
        assert_ne!(one.buf, both.buf);
        assert_eq!(
            language_title(&crate::i18n::Language::ALL),
            "Language / 语言"
        );
    }
}

#[cfg(test)]
mod volume_limit_tests {
    use super::*;

    /// Every row is on exactly one page: the home page or one topic page. A row on none could not
    /// be reached; a row on two would be drawn twice.
    #[test]
    fn every_row_is_on_exactly_one_page() {
        for r in 0..ROWS {
            let homes = HOME_ROWS.iter().filter(|x| **x == r).count();
            let groups = GROUPS
                .iter()
                .filter(|(_, _, rows)| rows.contains(&r))
                .count();
            assert_eq!(homes + groups, 1, "row {r} is on {} pages", homes + groups);
        }
    }

    /// Every row is reachable by a tap on its page, at scroll 0 or scrolled to it, in both sizes.
    #[test]
    fn every_row_is_hittable_on_its_page() {
        let _scale = crate::text::scale_guard();
        for large in [false, true] {
            crate::scale::set_large(large);
            for r in (0..ROWS).filter(|r| shown(*r, true)) {
                let page = page_of(r);
                let top = row_top_px(r) + LIST_TOP;
                let scroll = (top + RH() / 2 - (crate::canvas::H as i32 / 2))
                    .clamp(0, max_scroll_px(page, true));
                let y = top - scroll + RH() / 2;
                assert_eq!(
                    row_at(y, scroll, page, true),
                    Some(r),
                    "row {r} not hittable at y={y} scroll={scroll} (large {large})"
                );
            }
        }
        crate::scale::set_large(false);
    }

    /// The home page fits the glass without scrolling, and ends on the seven topic links.
    #[test]
    fn the_home_page_fits_and_lists_the_topics() {
        assert_eq!(
            max_scroll_px(None, true),
            0,
            "the home page must not scroll"
        );
        let rows = page_rows(None, true);
        assert_eq!(
            &rows[rows.len() - GROUPS.len()..],
            &(0..GROUPS.len()).map(group_row).collect::<Vec<_>>()[..]
        );
        for (g, group) in GROUPS.iter().enumerate() {
            assert_eq!(group_of_link(group_row(g)), Some(g));
            assert_eq!(page_of(group.2[0]), Some(g));
        }
        assert_eq!(group_of_link(ROW_MODEL), None);
    }

    /// #51: Big view's two extras sit under it and appear only while it is on.
    #[test]
    fn big_views_extras_show_only_with_big_view() {
        assert!(!shown(ROW_BIG_BAR, true) && !shown(ROW_BIG_ICONS, true));
        assert!(row_at(LIST_TOP + RH() * 2 + 1, 0, None, true) == Some(ROW_BRIGHTNESS));
        crate::scale::set_large(true);
        assert!(shown(ROW_BIG_BAR, true) && shown(ROW_BIG_ICONS, true));
        assert_eq!(
            row_at(LIST_TOP + RH() * 2 + 1, 0, None, true),
            Some(ROW_BIG_BAR)
        );
        crate::scale::set_large(false);
    }
}
