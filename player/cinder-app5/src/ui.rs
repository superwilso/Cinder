//! The UI half of v0.5: screen state, drawing and taps. Runs only on the shell's render/input
//! thread and reads the backend's latest snapshot; nothing here waits for the backend.

use crate::backend::Snapshot;
use cinder_core::display::Sink;
use cinder_ui::kit::Row;
use cinder_ui::now_playing::{Hit, NowPlaying};
use cinder_ui::np5::{self, Size, Tool};
use cinder_ui::sheet5::{self, SheetHit, Tile};
use cinder_ui::theme::Theme;
use cinder_ui::{Canvas, FontSet};

/// The shell's action codes (`cinder.h`, `cinder_action_t`) the UI returns.
pub const ACT_NONE: i32 = 0;
pub const ACT_PLAYPAUSE: i32 = 1;
pub const ACT_NEXT: i32 = 2;
pub const ACT_PREV: i32 = 3;
pub const ACT_VOLUP: i32 = 6;
pub const ACT_VOLDOWN: i32 = 7;

/// The shell's button codes (`cinder_button_t`) v0.5 answers so far.
const BTN_PLAY: i32 = 7;
const BTN_BACK: i32 = 5;
const BTN_VOLUP: i32 = 9;
const BTN_VOLDOWN: i32 = 10;
const BTN_NEXT: i32 = 13;
const BTN_PREV: i32 = 14;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sheet {
    Actions,
    Places,
}

pub struct Ui {
    pub sink: Option<Sink>,
    pub canvas: Canvas,
    pub fonts: FontSet,
    pub theme: Theme,
    /// The snapshot version last drawn; a newer one means repaint.
    pub drawn: u64,
    pub dirty: bool,
    pub size: Size,
    pub tools: Vec<Tool>,
    pub sheet: Option<Sheet>,
    /// The cover placeholder for (song, size), baked once: at 432 px it is past `art`'s cache and
    /// cost ~30 ms a frame drawn fresh.
    cover: Option<(String, Size, cinder_ui::art::Image)>,
}

impl Ui {
    pub fn new(sink: Option<Sink>) -> Ui {
        cinder_ui::chrome::set_bare_bar(true);
        Ui {
            sink,
            canvas: Canvas::new(),
            fonts: FontSet::load(),
            theme: Theme::day(),
            drawn: u64::MAX,
            dirty: true,
            size: Size::Big,
            tools: Tool::DEFAULT.to_vec(),
            sheet: None,
            cover: None,
        }
    }

    pub fn draw(&mut self, s: &Snapshot) {
        let edge = np5::cover_edge(self.size);
        if !matches!(&self.cover, Some((k, z, _)) if *k == s.title && *z == self.size) {
            let img = cinder_ui::art::gradient_image(&self.theme, edge, edge, &s.title, 1.0);
            self.cover = Some((s.title.clone(), self.size, img));
        }
        let mut np = now_playing(s);
        np.art_full = self.cover.as_ref().map(|c| &c.2);
        let (c, t, f) = (&mut self.canvas, &self.theme, &self.fonts);
        c.fill(t.bg);
        cinder_ui::scale::set_big_bar(self.size == Size::Huge);
        cinder_ui::chrome::status_bar(c, t, f, np.clock, np.badge, np.battery, None);
        cinder_ui::scale::set_big_bar(false);
        np5::render(c, t, f, &np, self.size, &self.tools);
        match self.sheet {
            Some(Sheet::Actions) => sheet5::song_actions(c, t, f, np.title, &ACTIONS),
            Some(Sheet::Places) => sheet5::places(c, t, f, &TILES, &LIST),
            None => {}
        }
    }

    /// A tap at UI coordinates. Returns a `cinder_action_t` for the shell.
    pub fn tap(&mut self, x: i32, y: i32) -> i32 {
        self.dirty = true;
        if let Some(sheet) = self.sheet {
            let hit = match sheet {
                Sheet::Actions => sheet5::song_actions_hit(ACTIONS.len(), y),
                Sheet::Places => sheet5::places_hit(TILES.len(), LIST.len(), x, y),
            };
            // ponytail: the sheets' items open screens v0.5 builds in P3-P5; until then they close.
            if hit != SheetHit::Panel {
                self.sheet = None;
            }
            return ACT_NONE;
        }
        match np5::layout(self.size, false, self.tools.len()).hit(x, y) {
            Some(Hit::PlayPause) => ACT_PLAYPAUSE,
            Some(Hit::Next) => ACT_NEXT,
            Some(Hit::Prev) => ACT_PREV,
            Some(Hit::Info) => {
                self.sheet = Some(Sheet::Actions);
                ACT_NONE
            }
            Some(Hit::Toolbar(i)) => {
                if self.tools.get(i as usize) == Some(&Tool::Places) {
                    self.sheet = Some(Sheet::Places);
                }
                ACT_NONE
            }
            _ => ACT_NONE,
        }
    }

    /// A horizontal swipe on Now Playing skips; on a sheet it does nothing.
    pub fn swipe(&mut self, dir: i32) -> i32 {
        match (self.sheet, dir < 0) {
            (Some(_), _) => ACT_NONE,
            (None, true) => ACT_NEXT,
            (None, false) => ACT_PREV,
        }
    }

    /// A hardware button.
    pub fn input(&mut self, button: i32) -> i32 {
        match button {
            BTN_PLAY => ACT_PLAYPAUSE,
            BTN_NEXT => ACT_NEXT,
            BTN_PREV => ACT_PREV,
            BTN_VOLUP => ACT_VOLUP,
            BTN_VOLDOWN => ACT_VOLDOWN,
            BTN_BACK if self.sheet.is_some() => {
                self.sheet = None;
                self.dirty = true;
                ACT_NONE
            }
            _ => ACT_NONE,
        }
    }
}

// ponytail: the sheets' content is the boards' until the backend owns the library (P3).
const ACTIONS: [Row; 6] = [
    Row::new("Lyrics"),
    Row::new("Track info and rating"),
    Row::new("Go to album"),
    Row::new("Add to playlist"),
    Row::new("Sleep timer"),
    Row::new("Output"),
];
const TILES: [Tile; 6] = [
    Tile {
        name: "Library",
        sub: "",
    },
    Tile {
        name: "Up Next",
        sub: "",
    },
    Tile {
        name: "Sound",
        sub: "",
    },
    Tile {
        name: "Bluetooth",
        sub: "",
    },
    Tile {
        name: "Wi-Fi",
        sub: "",
    },
    Tile {
        name: "Settings",
        sub: "",
    },
];
const LIST: [&str; 6] = [
    "Folders",
    "FM radio",
    "Soundscapes",
    "USB-DAC",
    "Audiobooks",
    "Help",
];

fn now_playing(s: &Snapshot) -> NowPlaying<'_> {
    NowPlaying {
        title: &s.title,
        artist: "",
        badge: "",
        clock: "",
        battery: s.battery,
        progress: s.progress,
        playing: s.playing,
        art: "kind",
        lyrics: false,
        liked: false,
        // ponytail: no times, codec or modes until the backend owns playback state (P2b).
        elapsed: "",
        remaining: "",
        codec: "",
        shuffle: false,
        repeat: 0,
        ..np5::sample()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_transport_answers_and_the_title_opens_this_songs_actions() {
        let mut u = Ui::new(None);
        let l = np5::layout(Size::Big, false, 4);
        let (x, y) = l.at(Hit::PlayPause).unwrap().centre();
        assert_eq!(u.tap(x, y), ACT_PLAYPAUSE);
        let (x, y) = l.at(Hit::Info).unwrap().centre();
        assert_eq!(u.tap(x, y), ACT_NONE);
        assert_eq!(u.sheet, Some(Sheet::Actions));
        assert_eq!(u.tap(240, 790), ACT_NONE, "Close");
        assert_eq!(u.sheet, None);
    }

    #[test]
    fn places_opens_from_the_bar_and_back_closes_it() {
        let mut u = Ui::new(None);
        assert_eq!(u.tap(420, 780), ACT_NONE);
        assert_eq!(u.sheet, Some(Sheet::Places));
        assert_eq!(u.input(BTN_BACK), ACT_NONE);
        assert_eq!(u.sheet, None);
        assert_eq!(u.input(BTN_NEXT), ACT_NEXT);
        assert_eq!(u.swipe(-1), ACT_NEXT);
    }
}
