//! Now Playing — ported from cinder-proto-screens1.jsx `CNowPlaying`.
//! Day: full-bleed 480x480 gradient art, 36-bar visualiser, title/artist/codec,
//! progress + time, transport (shuffle·prev·play·next·repeat), bottom toolbar.
//! Night: compact 92px thumb + text header, art dimmed to 32%, viz centred in
//! the negative space; shared progress / transport / toolbar below.

use crate::art;
use crate::canvas::Canvas;
use crate::icons;
use crate::text::{self, Family, FontSet, TextStyle, Weight};
use crate::theme::Theme;
use crate::widgets::{fill_rect, hline, right, sty};
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Circle, PrimitiveStyle};

// ── Progress-rail geometry — THE single source ────────────────────────────────────────────────
// Both renders below (loaded + idle) and the drag-to-seek hit test read these. The rail used to be
// literal numbers in three places; a scrub target that disagrees with the drawn bar by even a few
// pixels feels broken, so they live here and nowhere else.
pub const RAIL_Y: i32 = 612;
pub const RAIL_X0: i32 = 24;
pub const RAIL_W: i32 = 432;
/// Rail thickness. Single source for both the track and the filled portion.
pub const RAIL_H: i32 = 6;
/// Vertical grab band for drag-to-seek. The rail itself is 4 px tall — unhittable with a thumb —
/// so the band spans from just above the rail down through the elapsed/remaining labels. It stops
/// short of the transport row (centre y 692, radius 44 ⇒ from 648) so it can never steal a
/// play/pause tap.
pub const RAIL_GRAB_TOP: i32 = 594;
pub const RAIL_GRAB_BOT: i32 = 646;

/// Like (heart) glyph centre, and the half-size of its touch target. Single source for the draw
/// above and the hit test in nav.
/// The title / artist / codec block, as a tap target: it opens Track information.
///
/// Bounded ABOVE the progress rail's grab band and LEFT of the heart, both of which are tested
/// first — but the geometry excludes them anyway, so the order is belt and braces rather than the
/// thing keeping them apart. The title baseline is 558 and the artist/codec baseline 583, so the
/// band brackets both with room for a finger.
pub const INFO_TOP: i32 = 522;
pub const INFO_BOT: i32 = RAIL_GRAB_TOP - 1;

/// Is `(x, y)` on the metadata block? The right edge stops clear of the heart's square target.
/// The NIGHT layout's metadata block. The two themes are separate code paths and the block does
/// not live in the same place in both: the day layout puts title/artist/codec under the cover at
/// `INFO_TOP`, while night draws a compact header at the TOP of the screen (thumb at y=80, title
/// at 110, codec at 153).
///
/// The hit test used to be theme-blind, so in night mode the only way into Track information was
/// to tap a patch of EMPTY SPACE at y≈522 where the day layout's text would have been — nothing
/// there to suggest it, and the actual title, at the top, did nothing. Reported 2026-09-10 as
/// "in night mode there is no way to enter the track information screen".
///
/// The band is deliberately not left active in both places: a button in blank space is not a
/// feature, it is a thing you find by accident.
pub const INFO_NIGHT_TOP: i32 = 78;
pub const INFO_NIGHT_BOT: i32 = 174;

/// Is this tap on the metadata block — the "tell me more about this file" button?
///
/// `night` picks the layout, because the block moves with it.
pub fn hit_info(x: i32, y: i32, night: bool) -> bool {
    if night {
        // Full width: the thumb at x=24 is part of the same block, and nothing else on the night
        // header is tappable, so there is no neighbour to leave room for.
        return (INFO_NIGHT_TOP..=INFO_NIGHT_BOT).contains(&y)
            && (0..crate::canvas::W as i32).contains(&x);
    }
    (INFO_TOP..=INFO_BOT).contains(&y) && x >= 0 && x < HEART_CX - HEART_HALF - 2
}

/// The bottom toolbar's slot centres, and the single source for both the icons above and the hit
/// test in `nav::tap`. Four 120 px slots across a 480 px panel (library, queue, bluetooth, settings).
pub const TOOLBAR_TOP: i32 = 744;
pub const TOOLBAR_SLOTS: usize = 4;
pub const TOOLBAR_CX: [i32; TOOLBAR_SLOTS] = [60, 180, 300, 420];

pub const HEART_CX: i32 = 432;
pub const HEART_CY: i32 = 548;
pub const HEART_HALF: i32 = 30;

/// Map a UI x coordinate to a 0..1 position along the rail (clamped). Used by the scrub.
pub fn rail_fraction(x: i32) -> f32 {
    ((x - RAIL_X0) as f32 / RAIL_W as f32).clamp(0.0, 1.0)
}

// ── The layout: one pure function per style, read by the draw AND the tap ─────────────────────
//
// `layout` answers "what is where" for Now Playing without drawing anything, so `nav::tap`, the
// scrub and the page swipe can ask it before any frame has been painted (the headless tests do
// exactly that), and each style's render draws its controls from the same numbers. Before this,
// the transport row was literal circles in `nav.rs` — `hit(130, 692, 34)` for a Prev icon drawn at
// 128 — which is the drift this exists to end. (`docs/PLAN_skins.md`, `docs/PLAN_2026-09-14.md` C2.)

/// Everything on Now Playing a tap can mean.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Hit {
    Lyrics,
    Like,
    /// The title block: opens Track information.
    Info,
    PlayPause,
    Prev,
    Next,
    Shuffle,
    Repeat,
    /// Library, Up Next, Bluetooth, Settings.
    Toolbar(u8),
    /// The band under the status bar: opens the Menu.
    Menu,
}

/// A target's shape. Circles for round controls, so a corner tap beside a round button is not
/// taken by it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Shape {
    Rect { x: i32, y: i32, w: i32, h: i32 },
    Circle { cx: i32, cy: i32, r: i32 },
}

impl Shape {
    pub fn contains(self, px: i32, py: i32) -> bool {
        match self {
            Shape::Rect { x, y, w, h } => px >= x && px < x + w && py >= y && py < y + h,
            Shape::Circle { cx, cy, r } => (px - cx).pow(2) + (py - cy).pow(2) <= r * r,
        }
    }

    /// The bounding box, `(x, y, w, h)`.
    pub fn bounds(self) -> (i32, i32, i32, i32) {
        match self {
            Shape::Rect { x, y, w, h } => (x, y, w, h),
            Shape::Circle { cx, cy, r } => (cx - r, cy - r, 2 * r + 1, 2 * r + 1),
        }
    }

    pub fn centre(self) -> (i32, i32) {
        let (x, y, w, h) = self.bounds();
        (x + w / 2, y + h / 2)
    }
}

/// The seek rail: where it is drawn, and the taller band a finger grabs it by.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rail {
    pub x0: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub grab_top: i32,
    pub grab_bot: i32,
}

impl Rail {
    /// A finger down at `(x, y)` takes the rail.
    pub fn grabs(&self, x: i32, y: i32) -> bool {
        (self.grab_top..=self.grab_bot).contains(&y) && (0..=crate::canvas::W as i32).contains(&x)
    }

    /// `x` as a 0..1 position along the rail, clamped.
    pub fn fraction(&self, x: i32) -> f32 {
        ((x - self.x0) as f32 / self.w.max(1) as f32).clamp(0.0, 1.0)
    }
}

/// Most targets a Now Playing layout has. A fixed array, not a `Vec`: the styles' renders ask for
/// their layout every frame, and this device has aborted an allocator over render-path churn.
pub const MAX_REGIONS: usize = 16;

/// What is where on Now Playing, for one style, mode and song.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Layout {
    regions: [(Shape, Hit); MAX_REGIONS],
    len: usize,
    pub rail: Rail,
    /// The block a horizontal swipe turns the page in (cover, spectrum, level). Below it, a swipe
    /// skips tracks.
    pub page_top: i32,
    pub page_bot: i32,
}

impl Layout {
    pub fn new(rail: Rail, page_top: i32, page_bot: i32) -> Layout {
        let none = (
            Shape::Rect {
                x: 0,
                y: 0,
                w: 0,
                h: 0,
            },
            Hit::Menu,
        );
        Layout {
            regions: [none; MAX_REGIONS],
            len: 0,
            rail,
            page_top,
            page_bot,
        }
    }

    /// Add a target. Earlier targets win where two overlap. Past `MAX_REGIONS` the rest are
    /// dropped, and `every_style_keeps_the_contract` fails.
    pub fn push(&mut self, shape: Shape, hit: Hit) {
        if self.len < MAX_REGIONS {
            self.regions[self.len] = (shape, hit);
            self.len += 1;
        }
    }

    /// Every target, in the order a tap is tested against them.
    pub fn regions(&self) -> &[(Shape, Hit)] {
        &self.regions[..self.len]
    }

    /// What a tap at `(x, y)` means.
    pub fn hit(&self, x: i32, y: i32) -> Option<Hit> {
        self.regions()
            .iter()
            .find(|(s, _)| s.contains(x, y))
            .map(|(_, h)| *h)
    }

    /// Where `hit` is, if this layout has it.
    pub fn at(&self, hit: Hit) -> Option<Shape> {
        self.regions()
            .iter()
            .find(|(_, h)| *h == hit)
            .map(|(s, _)| *s)
    }
}

/// Now Playing's layout in `style`. `night` matters to Cinder, whose night layout is a different
/// screen; `lyrics` adds the Lyrics chip's target when the song has lyrics.
pub fn layout(style: crate::style::Style, night: bool, lyrics: bool) -> Layout {
    match style {
        crate::style::Style::Cinder => cinder_layout(night, lyrics),
        other => crate::np_styles::layout(other, lyrics),
    }
}

/// The transport row, as Cinder draws it. The render and `cinder_layout` both read these.
pub const TRANSPORT_Y: i32 = 692;
pub const PLAY_R: i32 = 44;
pub const PREV_X: i32 = 128;
pub const NEXT_X: i32 = 352;
pub const SIDE_R: i32 = 34;
pub const SHUFFLE_X: i32 = 44;
pub const REPEAT_X: i32 = 436;
pub const SMALL_R: i32 = 30;

/// Cinder's own Now Playing, as it has always answered taps: the Lyrics chip first (its band is
/// otherwise the Menu's), the heart, the title block, the transport, the toolbar, then the Menu
/// band. The order is the priority where two overlap.
fn cinder_layout(night: bool, lyrics: bool) -> Layout {
    let w = crate::canvas::W as i32;
    let rail = Rail {
        x0: RAIL_X0,
        y: RAIL_Y,
        w: RAIL_W,
        h: RAIL_H,
        grab_top: RAIL_GRAB_TOP,
        grab_bot: RAIL_GRAB_BOT,
    };
    let mut l = Layout::new(rail, PAGE_TOP, PAGE_SWIPE_BOT);
    if lyrics {
        l.push(
            Shape::Rect {
                x: 0,
                y: crate::chrome::STATUS_H,
                w: LYRICS_HIT_W,
                h: 44,
            },
            Hit::Lyrics,
        );
    }
    l.push(
        Shape::Rect {
            x: HEART_CX - HEART_HALF,
            y: HEART_CY - HEART_HALF,
            w: 2 * HEART_HALF + 1,
            h: 2 * HEART_HALF + 1,
        },
        Hit::Like,
    );
    let info = if night {
        Shape::Rect {
            x: 0,
            y: INFO_NIGHT_TOP,
            w,
            h: INFO_NIGHT_BOT - INFO_NIGHT_TOP + 1,
        }
    } else {
        Shape::Rect {
            x: 0,
            y: INFO_TOP,
            w: HEART_CX - HEART_HALF - 2,
            h: INFO_BOT - INFO_TOP + 1,
        }
    };
    l.push(info, Hit::Info);
    let ty = TRANSPORT_Y;
    l.push(
        Shape::Circle {
            cx: 240,
            cy: ty,
            r: PLAY_R,
        },
        Hit::PlayPause,
    );
    l.push(
        Shape::Circle {
            cx: PREV_X,
            cy: ty,
            r: SIDE_R,
        },
        Hit::Prev,
    );
    l.push(
        Shape::Circle {
            cx: NEXT_X,
            cy: ty,
            r: SIDE_R,
        },
        Hit::Next,
    );
    l.push(
        Shape::Circle {
            cx: SHUFFLE_X,
            cy: ty,
            r: SMALL_R,
        },
        Hit::Shuffle,
    );
    l.push(
        Shape::Circle {
            cx: REPEAT_X,
            cy: ty,
            r: SMALL_R,
        },
        Hit::Repeat,
    );
    let slot = w / TOOLBAR_SLOTS as i32;
    for i in 0..TOOLBAR_SLOTS {
        let y = TOOLBAR_TOP + 1;
        l.push(
            Shape::Rect {
                x: i as i32 * slot,
                y,
                w: slot,
                h: crate::canvas::H as i32 - y,
            },
            Hit::Toolbar(i as u8),
        );
    }
    l.push(
        Shape::Rect {
            x: 0,
            y: 0,
            w,
            h: crate::chrome::HEADER_BOTTOM,
        },
        Hit::Menu,
    );
    l
}

#[derive(Clone, Copy)]
pub struct NowPlaying<'a> {
    pub title: &'a str,
    pub artist: &'a str,
    pub codec: &'a str, // "FLAC · 24bit / 96.0 kHz"
    pub badge: &'a str, // status-bar badge "FLAC 24/96"
    pub clock: &'a str,
    pub battery: u8,
    pub elapsed: &'a str,
    pub remaining: &'a str,
    pub progress: f32, // 0..1
    pub art: &'a str,  // swatch name (gradient fallback when no decoded cover)
    /// Real decoded cover art, pre-scaled by the shell: full-bleed 480×480 (day) and the
    /// 92×92 thumb (night header). None = draw the gradient fallback.
    pub art_full: Option<&'a art::Image>,
    pub art_thumb: Option<&'a art::Image>,
    pub liked: bool,
    pub playing: bool,
    pub shuffle: bool,
    /// Repeat: 0 = off, 1 = repeat-one, 2 = repeat-all. Repeat-all was absent for a long time
    /// because PlayerService exposes no primitive for it — and a third position that changed
    /// nothing would have been the same lie the shuffle icon used to tell. It is real now: the
    /// queue boundary is detectable (position pins at duration, `playing` goes 1 -> 0, URI
    /// unchanged — DEVICE_TESTS.md 3f), so the shell re-issues the queue itself.
    /// The glyph needs no new art: `> 0` accents it and `== 1` adds the "one" dot. 3 = repeat
    /// ALBUM (the run of the list the playing album occupies; the shell laps it), captioned ALBUM.
    pub repeat: u8,
    pub viz_seed: f32, // visualiser animation phase (the shell advances it while playing)
    pub viz_kind: u8,  // which visualiser type (index into viz::from_index)
    /// How much room the visualiser gets, as a `viz::VizSize` index (0 = OFF). Replaced the old
    /// on/off flag: on the day theme the visualiser is drawn OVER the album art, so "how much"
    /// is the question that actually matters, and off is just the smallest answer.
    pub viz_size: u8,
    pub viz_levels: Option<&'a [f32]>, // real per-bar spectrum (0..1); None = no analyzer, synthetic motion
    /// Peak-hold markers, one per bar, or None when the user has them switched off. Separate from
    /// `viz_levels` because a marker is deliberately NOT smoothed the way a bar is.
    pub viz_peaks: Option<&'a [f32]>,
    /// The decoded audio itself, for the styles that need more than band levels (Scope, Stereo
    /// field, Meters, Spectrogram). None when the shell has none to give.
    pub viz_sig: Option<&'a crate::viz::Signal<'a>>,
    /// Which Now Playing PAGE is showing (index into `NpPage`). Only the block above the title
    /// changes — the title, progress, transport and toolbar are identical on every page, so the
    /// controls never move under your thumb.
    pub page: u8,
    /// A drag-to-seek is in progress: `progress`/`elapsed`/`remaining` show the pending TARGET
    /// rather than the live position, and the rail grows a handle under the finger.
    pub scrubbing: bool,
    /// The playing song has lyrics: draw the Lyrics chip ([`hit_lyrics`]). Injected by `nav`,
    /// which owns the parsed lyrics; the shell passes `false`.
    pub lyrics: bool,
}

/// The pages you swipe between on Now Playing. The visualiser used to be painted ON the cover,
/// which meant every choice was a compromise between seeing the artwork and seeing the audio. As
/// pages they stop competing: the cover page is the cover, and the visualiser gets a whole block
/// to itself instead of a strip.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NpPage {
    /// The album cover, full bleed. Optionally with a small visualiser (see `viz_size`).
    Cover,
    /// The spectrum, given the entire art block. Style follows the Visualiser type setting.
    Spectrum,
    /// Output level: one large meter with a peak marker. No per-band detail — the calm page.
    Level,
}

pub const PAGES: u8 = 3;

pub fn page_from_index(i: u8) -> NpPage {
    match i % PAGES {
        0 => NpPage::Cover,
        1 => NpPage::Spectrum,
        _ => NpPage::Level,
    }
}

/// The block that pages: the full-bleed art area on the day theme.
pub const PAGE_TOP: i32 = 34;
pub const PAGE_BOT: i32 = 514;
/// A horizontal swipe ABOVE this y flips the page; below it, it still skips tracks. Splitting the
/// gesture by zone keeps both: you swipe the artwork to turn it, and the controls area to change
/// what is playing. (The physical FF/REW keys skip from anywhere regardless, and they are the
/// primary skip affordance on a device with no d-pad.)
pub const PAGE_SWIPE_BOT: i32 = PAGE_BOT;

fn s(
    fam: Family,
    weight: Weight,
    size: f32,
    color: embedded_graphics::pixelcolor::Rgb888,
    tracking: f32,
) -> TextStyle {
    sty(fam, weight, size, color, tracking)
}

/// Small accent "SLEEP {n}M" badge, top-right under the status bar, shown while a sleep timer runs.
/// Drawn by the navigator AFTER render() (it owns the live countdown) — kept here to share the
/// screen's draw imports. `min` = 0 hides it.
/// `NowPlaying::repeat` for repeat album. 0 off, 1 one, 2 all.
pub const REPEAT_ALBUM: u8 = 3;

pub fn sleep_badge(c: &mut Canvas, t: &Theme, f: &FontSet, min: u32, end_of_song: bool) {
    let label = if end_of_song {
        "SLEEP AT SONG END".to_string()
    } else if min == 0 {
        return;
    } else {
        format!("SLEEP {}M", min)
    };
    let st = s(Family::Mono, Weight::Bold, 12.0, t.acc_ink, 0.08);
    let w = text::measure(f, &label, &st) as i32 + 22;
    let h = 24;
    let x = 458 - w;
    let y = 44;
    fill_rect(c, x, y, w, h, t.acc);
    text::draw(c, f, (x + 11) as f32, (y + h / 2 + 4) as f32, &label, &st);
}

/// The Lyrics chip — one tap to the Lyrics screen, which is otherwise two taps deep (Track
/// information ▸ Lyrics). Asked for on the r/walkman thread by someone who embeds lyrics in every
/// file (`docs/PLAN_community_2026-09-23.md` B2).
///
/// Drawn ONLY when the playing song has lyrics, so a library without them sees exactly the screen
/// it always did. It sits top-LEFT of the paging block because the sleep-timer badge owns the
/// top-right corner. The pill is 24 px tall; the target is the full 44 px band under the status
/// bar and wider than the pill, which is the minimum a finger needs. In that band a tap otherwise
/// opens the Menu (see `nav::tap`), so the chip is tested first.
pub const LYRICS_X0: i32 = 16;
pub const LYRICS_Y0: i32 = crate::chrome::STATUS_H;
/// The same height as the sleep badge opposite it, so the two corners read as one row — and at
/// night the pill ends well clear of the thumbnail at y=80.
pub const LYRICS_H: i32 = 24;
const LYRICS_LABEL: &str = "LYRICS";
/// The target's width: generous, and independent of how wide the text measured, so the UI scale
/// cannot move it.
pub const LYRICS_HIT_W: i32 = 120;

fn lyrics_style(t: &Theme) -> TextStyle {
    s(
        Family::Mono,
        Weight::Regular,
        crate::scale::CAPTION,
        t.acc,
        0.14,
    )
}

/// Did a tap land on the Lyrics chip? `shown` = the chip is drawn (the song has lyrics).
pub fn hit_lyrics(x: i32, y: i32, shown: bool) -> bool {
    shown
        && (0..LYRICS_HIT_W).contains(&x)
        && (crate::chrome::STATUS_H..crate::chrome::STATUS_H + 44).contains(&y)
}

pub(crate) fn lyrics_chip(c: &mut Canvas, t: &Theme, f: &FontSet) {
    let st = lyrics_style(t);
    let label = crate::widgets::fit(f, LYRICS_LABEL, &st, (LYRICS_HIT_W - 24 - LYRICS_X0) as f32);
    let w = text::measure(f, &label, &st) as i32 + 24;
    fill_rect(c, LYRICS_X0, LYRICS_Y0, w, LYRICS_H, t.panel);
    crate::widgets::stroke_rect(c, LYRICS_X0, LYRICS_Y0, w, LYRICS_H, t.ctrl(), 1);
    text::draw(
        c,
        f,
        (LYRICS_X0 + 12) as f32,
        (LYRICS_Y0 + LYRICS_H / 2 + 5) as f32,
        &label,
        &st,
    );
}

/// Page indicator: one dot per page, in the strip between the paging block and the title. Small
/// and faint — it is a "there is more this way" hint, not a control. Without it the pages would be
/// undiscoverable, which is the usual way a swipe-only feature ends up never being found.
fn page_dots(c: &mut Canvas, t: &Theme, page: u8) {
    const D: i32 = 6;
    const GAP: i32 = 10;
    let total = PAGES as i32 * D + (PAGES as i32 - 1) * GAP;
    let x0 = 240 - total / 2;
    for i in 0..PAGES {
        let x = x0 + i as i32 * (D + GAP);
        let col = if i == page % PAGES { t.acc } else { t.faint };
        fill_rect(c, x, 524, D, D, col);
    }
}

/// Mean and peak of the current spectrum, 0..1. `None` levels (no analyzer running) give zeros,
/// so both audio pages render flat and empty rather than inventing motion.
pub(crate) fn level_stats(np: &NowPlaying) -> (f32, f32) {
    match np.viz_levels {
        Some(l) if !l.is_empty() => {
            let sum: f32 = l.iter().sum();
            let peak = l.iter().cloned().fold(0.0f32, f32::max);
            ((sum / l.len() as f32).clamp(0.0, 1.0), peak.clamp(0.0, 1.0))
        }
        _ => (0.0, 0.0),
    }
}

/// Can the spectrum page draw the chosen style right now?
pub(crate) fn viz_live(np: &NowPlaying) -> bool {
    crate::viz::can_draw(
        crate::viz::from_index(np.viz_kind),
        np.viz_levels,
        np.viz_sig,
    )
}

/// What the spectrum page says instead, `(headline, caption)`. Music is playing but the style needs
/// the decoded samples, which only library playback provides: say that, not "no signal" — there IS
/// one, and the other styles are drawing it.
pub(crate) fn viz_absent_text(np: &NowPlaying) -> (&'static str, &'static str) {
    let kind = crate::viz::from_index(np.viz_kind);
    if np.viz_levels.is_some() && crate::viz::needs_samples(kind) {
        (
            "Needs library playback",
            "FM, USB-DAC AND THE RECEIVER GIVE BANDS ONLY",
        )
    } else if np.viz_levels.is_some() {
        ("Gathering", "THE PICTURE BUILDS AS THE MUSIC PLAYS")
    } else {
        ("No audio signal", "PLAY SOMETHING TO SEE THE SPECTRUM")
    }
}

/// PAGE 2 — the spectrum, given the whole block instead of a strip. Same styles as the cover
/// overlay, just with room: this is where a visualiser is worth looking at.
fn spectrum_page(c: &mut Canvas, t: &Theme, f: &FontSet, np: &NowPlaying, seed: f32) {
    let (x, w) = (24, 432);
    let (y, h) = (154, 348); // stands at 502, clear of the page dots at 524
    if viz_live(np) {
        crate::viz::draw_any(
            c,
            x,
            y,
            w,
            h,
            seed,
            crate::viz::from_index(np.viz_kind),
            t.acc,
            t.line,
            np.viz_levels,
            np.viz_peaks,
            np.viz_sig,
            255,
            255,
        );
    } else {
        // No analyzer feeding us. Say so rather than drawing a still, empty graph that reads as a
        // broken screen — the same rule the rest of the app follows about showing what isn't there.
        let (head, cap) = viz_absent_text(np);
        crate::widgets::center(
            c,
            f,
            240.0,
            330.0,
            head,
            &s(Family::Sans, Weight::Regular, 20.0, t.dim, 0.0),
        );
        crate::widgets::center(
            c,
            f,
            240.0,
            356.0,
            cap,
            &s(Family::Mono, Weight::Regular, 11.0, t.faint, 0.18),
        );
    }
    crate::widgets::center(
        c,
        f,
        240.0,
        130.0,
        crate::viz::name_upper(np.viz_kind),
        &s(Family::Mono, Weight::Regular, 11.0, t.faint, 0.18),
    );
}

/// PAGE 2, night layout. Same page, different block: the compact header occupies the top ~160px,
/// so the spectrum takes the open space beneath it rather than a cover's footprint.
fn spectrum_page_night(c: &mut Canvas, t: &Theme, f: &FontSet, np: &NowPlaying, seed: f32) {
    let (x, w) = (24, 432);
    let (y, h) = (220, 260); // stands at 480, clear of the page dots
    if viz_live(np) {
        crate::viz::draw_any(
            c,
            x,
            y,
            w,
            h,
            seed,
            crate::viz::from_index(np.viz_kind),
            t.acc,
            t.line,
            np.viz_levels,
            np.viz_peaks,
            np.viz_sig,
            255,
            255,
        );
    } else {
        crate::widgets::center(
            c,
            f,
            240.0,
            340.0,
            viz_absent_text(np).0,
            &s(Family::Sans, Weight::Regular, 20.0, t.dim, 0.0),
        );
    }
    crate::widgets::center(
        c,
        f,
        240.0,
        196.0,
        crate::viz::name_upper(np.viz_kind),
        &s(Family::Mono, Weight::Regular, 11.0, t.faint, 0.18),
    );
}

/// Decimal for 0..=999 into a caller-supplied buffer. Allocation-free, and clamped rather than
/// fallible so a nonsense level can never panic a render (a panic here aborts, and an abort on this
/// device is a reboot into stock).
pub(crate) fn dec(v: i32, buf: &mut [u8; 3]) -> &str {
    let v = v.clamp(0, 999) as u32;
    let mut n = 0;
    if v >= 100 {
        buf[n] = b'0' + (v / 100) as u8;
        n += 1;
    }
    if v >= 10 {
        buf[n] = b'0' + (v / 10 % 10) as u8;
        n += 1;
    }
    buf[n] = b'0' + (v % 10) as u8;
    n += 1;
    core::str::from_utf8(&buf[..n]).unwrap_or("0")
}

/// "PEAK nnn", allocation-free.
fn peak_label(v: i32, buf: &mut [u8; 8]) -> &str {
    buf[..5].copy_from_slice(b"PEAK ");
    let mut d = [0u8; 3];
    let s = dec(v, &mut d);
    let n = 5 + s.len();
    buf[5..n].copy_from_slice(s.as_bytes());
    core::str::from_utf8(&buf[..n]).unwrap_or("PEAK")
}

/// PAGE 3 — output level. One big meter, a peak marker, and a scale. No per-band detail: this is
/// the page for when you want to see that it is playing without anything asking for attention.
fn level_page(c: &mut Canvas, t: &Theme, f: &FontSet, np: &NowPlaying) {
    let (mean, peak) = level_stats(np);
    let (x, w) = (36, 408);
    let (y, h) = (270, 64);

    // The night layout puts the track header at the top of the screen, where the day layout has
    // artwork — so this caption has to move out from under it rather than sit at a fixed y.
    let label_y = if t.night { 200.0 } else { 130.0 };
    crate::widgets::center(
        c,
        f,
        240.0,
        label_y,
        "OUTPUT LEVEL",
        &s(Family::Mono, Weight::Regular, 11.0, t.faint, 0.18),
    );

    // Track, then fill. A visible empty track is what makes the fill mean something — a bare bar
    // on a black screen has no scale to be read against.
    fill_rect(c, x, y, w, h, t.panel);
    crate::widgets::stroke_rect(c, x, y, w, h, t.line, 1);
    let fw = (w as f32 * mean).round() as i32;
    if fw > 0 {
        fill_rect(c, x, y, fw, h, t.acc);
    }
    // Peak marker: a 3px rule at the loudest band. Peak sits at or right of the mean by definition,
    // so it never hides inside the fill.
    let px = x + ((w - 3) as f32 * peak).round() as i32;
    fill_rect(
        c,
        px,
        y - 8,
        3,
        h + 16,
        if peak > 0.0 { t.ink } else { t.line },
    );

    // Scale ticks under the meter, at tenths. Every fifth is full height.
    for i in 0..=10 {
        let tx = x + (w - 1) * i / 10;
        let th = if i % 5 == 0 { 10 } else { 5 };
        fill_rect(c, tx, y + h + 8, 1, th, t.faint);
    }

    // The numbers, big, in the space below. Mono so they do not jitter as the digits change —
    // proportional figures would make the whole line dance at 20 fps.
    // Stack-formatted, not `format!`. This page redraws at ~20 fps while playing, and two heap
    // allocations a frame is churn this device has already been bitten by once — the per-frame
    // Canvas allocation that ended in an allocator abort, and an abort here means a reboot.
    let mut mb = [0u8; 3];
    crate::widgets::center(
        c,
        f,
        240.0,
        430.0,
        dec((mean * 100.0).round() as i32, &mut mb),
        &s(Family::Mono, Weight::Bold, 56.0, t.ink, 0.0),
    );
    let mut pb = [0u8; 8];
    crate::widgets::center(
        c,
        f,
        240.0,
        460.0,
        peak_label((peak * 100.0).round() as i32, &mut pb),
        &s(Family::Mono, Weight::Regular, 12.0, t.faint, 0.14),
    );
}

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, np: &NowPlaying) {
    if t.style != crate::style::Style::Cinder {
        crate::np_styles::render(c, t, f, np);
        return;
    }
    c.fill(t.bg);
    let seed = np.viz_seed; // animated by the shell while playing; constant when paused/host

    // Nothing loaded — the state the device sits in from boot until the first track is picked.
    // Falling through would draw an art block seeded from an empty string (an orphan coloured
    // square) above three empty text runs, which is what the device actually showed.
    if np.title.is_empty() && np.artist.is_empty() {
        let cy = if t.night { 120.0 } else { 274.0 };
        crate::widgets::center(
            c,
            f,
            240.0,
            cy,
            "Nothing playing",
            &s(Family::Sans, Weight::Regular, 22.0, t.dim, 0.0),
        );
        crate::widgets::center(
            c,
            f,
            240.0,
            cy + 26.0,
            "CHOOSE A TRACK FROM YOUR LIBRARY",
            &s(Family::Mono, Weight::Regular, 11.0, t.faint, 0.18),
        );
        idle_chrome(c, t, f);
        return;
    }

    if t.night {
        // compact header: 92px thumb @32% + title/artist/codec column
        match np.art_thumb {
            Some(img) => art::draw_image(c, t, 24, 80, img, 0.32),
            None => art::block_cached(c, t, 24, 80, 92, 92, np.art, 0.32),
        }
        // TRUNCATE. The day layout below already fits its title to 372px; this column never did,
        // so in night mode a long title or a non-Latin artist ran straight off the right edge and
        // was clipped away — 6000+ pixels of it on a real classical tag. Caught by
        // tests/ui_overflow.rs; the two layouts are separate code paths and only one had the bound.
        const COL_X: f32 = 134.0;
        const COL_W: f32 = 456.0 - COL_X;
        let ts = s(Family::Sans, Weight::Bold, 23.0, t.ink, 0.0);
        let as_ = s(Family::Sans, Weight::Regular, 16.0, t.dim, 0.0);
        let cs = s(Family::Mono, Weight::Regular, 12.0, t.acc, 0.08);
        // Same rule as the day layout: this column is the title, so it scrolls rather than
        // truncating. It was the tighter of the two boxes to begin with (COL_W against 372px).
        crate::widgets::marquee(c, f, COL_X, 110.0, np.title, &ts, COL_W);
        crate::widgets::marquee(c, f, COL_X, 133.0, np.artist, &as_, COL_W);
        text::draw(
            c,
            f,
            COL_X,
            153.0,
            &crate::widgets::fit(f, np.codec, &cs, COL_W),
            &cs,
        );
        // Night pages the same way the day theme does, but the block is different: there is no
        // full-bleed cover to page, only the airy negative space under the compact header. So the
        // header stays put and the SPACE changes. Swiping still works, the dots still say where you
        // are, and nothing about the gesture has to be relearned when the theme flips.
        //
        // Everything here inherits the night palette, whose accent is already taken down to ~55%
        // luminance — so the spectrum page at night is a dim spectrum, not a bright one. That is
        // the point of the theme and the visualiser does not get an exemption from it.
        match page_from_index(np.page) {
            NpPage::Cover => {
                // The small visualiser, standing on y=436 in the open space — over nothing, so at
                // night this size axis is about restraint rather than about hiding artwork.
                if let Some((vy, vh, at, ab)) =
                    crate::viz::size_box(crate::viz::size_from_index(np.viz_size), 436, true)
                {
                    crate::viz::draw_any(
                        c,
                        24,
                        vy,
                        432,
                        vh,
                        seed,
                        crate::viz::from_index(np.viz_kind),
                        t.acc,
                        t.line,
                        np.viz_levels,
                        np.viz_peaks,
                        np.viz_sig,
                        at,
                        ab,
                    );
                }
            }
            NpPage::Spectrum => spectrum_page_night(c, t, f, np, seed),
            NpPage::Level => level_page(c, t, f, np),
        }
        page_dots(c, t, np.page);
    } else {
        // The PAGING BLOCK (34..514). Only this changes between pages; everything below it is
        // identical on every page, so the transport never moves under your thumb when you turn one.
        match page_from_index(np.page) {
            NpPage::Cover => {
                // full-bleed album art: the real decoded cover when available
                match np.art_full {
                    Some(img) => art::draw_image(c, t, 0, 34, img, 1.0),
                    // 480x480 is past the cache's edge limit, so this still bakes per frame — but
                    // only when the shell supplied no image at all, which on device it always does
                    // (cinder-ffi bakes one per track). See art::block_cached.
                    None => art::block_cached(c, t, 0, 34, 480, 480, np.art, 1.0),
                }
                // The visualiser stands on the BOTTOM EDGE of the cover (y=508, six px clear of
                // the art's real edge at 514) and grows upward into it, so changing size moves
                // only its top and the cover's composition below never shifts.
                let vsize = crate::viz::size_from_index(np.viz_size);
                if let Some((vy, vh, at, ab)) = crate::viz::size_box(vsize, 508, false) {
                    crate::viz::draw_any(
                        c,
                        24,
                        vy,
                        432,
                        vh,
                        seed,
                        crate::viz::from_index(np.viz_kind),
                        t.acc,
                        t.line,
                        np.viz_levels,
                        np.viz_peaks,
                        np.viz_sig,
                        at,
                        ab,
                    );
                }
            }
            NpPage::Spectrum => spectrum_page(c, t, f, np, seed),
            NpPage::Level => level_page(c, t, f, np),
        }
        page_dots(c, t, np.page);
        // title / artist / codec
        //
        // The title SCROLLS rather than being cut off at 372px. This is the one line on the device
        // whose whole content matters — it is the answer to "what is this?" — and a classical or
        // remix title loses exactly the distinguishing part to an ellipsis. Everything shorter
        // than the box is drawn identically to before and costs nothing extra.
        let tst = s(Family::Sans, Weight::Bold, 29.0, t.ink, 0.0);
        crate::widgets::marquee(c, f, 24.0, 558.0, np.title, &tst, 372.0);
        // Artist (left) and codec (right) share this baseline, so they are laid out against each
        // other rather than against two fixed x values — at 140% the artist used to run straight
        // through the codec string. The codec keeps its full width and the artist marquees in
        // whatever is left, which is the same precedence `row_pair` already applies — the value is
        // short and fixed, the name is the one that can be arbitrarily long.
        let ast = s(Family::Sans, Weight::Regular, 17.0, t.dim, 0.0);
        let cst = s(Family::Mono, Weight::Regular, 12.0, t.acc, 0.08);
        let cw = if np.codec.is_empty() {
            0.0
        } else {
            text::measure(f, np.codec, &cst)
        };
        crate::widgets::marquee(
            c,
            f,
            24.0,
            583.0,
            np.artist,
            &ast,
            (456.0 - cw - 12.0 - 24.0).max(0.0),
        );
        if !np.codec.is_empty() {
            crate::widgets::right(c, f, 456.0, 583.0, np.codec, &cst);
        }
    }

    if np.lyrics {
        lyrics_chip(c, t, f);
    }

    // ---------- like (heart) ----------
    // Wired at last: `liked` and icons::heart existed but nothing ever drew the glyph, so the
    // field was carried through four crates for an invisible feature. Sits on the title row, which
    // is where the eye already is and clear of every transport target.
    icons::heart(
        c,
        HEART_CX as f32,
        HEART_CY as f32,
        26.0,
        if np.liked { t.acc } else { t.faint },
    );

    // ---------- progress (shared) ----------
    let (py, px0, pw) = (RAIL_Y, RAIL_X0, RAIL_W);
    fill_rect(c, px0, py, pw, RAIL_H, t.line);
    let fillw = (pw as f32 * np.progress.clamp(0.0, 1.0)) as i32;
    fill_rect(c, px0, py, fillw, RAIL_H, t.acc);
    // Scrub handle: only while a drag-to-seek is in progress. It gives the finger something to
    // aim at and makes it obvious the bar is showing a pending target, not the live position.
    if np.scrubbing {
        let cx = px0 + fillw;
        Circle::with_center(Point::new(cx, py + RAIL_H / 2), 22)
            .into_styled(PrimitiveStyle::with_fill(t.acc))
            .draw(c)
            .ok();
    }
    text::draw(
        c,
        f,
        24.0,
        636.0,
        np.elapsed,
        &s(Family::Mono, Weight::Regular, 13.0, t.dim, 0.0),
    );
    right(
        c,
        f,
        456.0,
        636.0,
        np.remaining,
        &s(Family::Mono, Weight::Regular, 13.0, t.faint, 0.0),
    );

    // ---------- transport (centre y 692, larger controls) ----------
    let ty = TRANSPORT_Y as f32;
    icons::shuffle(
        c,
        SHUFFLE_X as f32,
        ty,
        24.0,
        if np.shuffle { t.acc } else { t.faint },
    );
    icons::prev(c, PREV_X as f32, ty, 38.0, t.ink);
    Circle::with_center(Point::new(240, ty as i32), 92)
        .into_styled(PrimitiveStyle::with_fill(t.acc))
        .draw(c)
        .ok();
    if np.playing {
        icons::pause(c, 240.0, ty, 38.0, t.acc_ink);
    } else {
        icons::play(c, 240.0, ty, 38.0, t.acc_ink);
    }
    icons::next(c, NEXT_X as f32, ty, 38.0, t.ink);
    icons::repeat(
        c,
        REPEAT_X as f32,
        ty,
        24.0,
        if np.repeat > 0 { t.acc } else { t.faint },
    );
    if np.repeat == 1 {
        // the "one" dot inside the loop glyph
        fill_rect(c, 435, ty as i32 - 1, 3, 3, t.acc);
    } else if np.repeat == REPEAT_ALBUM {
        // Named under the glyph: an A inside a 24 px loop is too small to read at arm's length,
        // and the transport row has 30 px of free space beneath it before the toolbar rule.
        crate::widgets::center(
            c,
            f,
            436.0,
            ty + 27.0,
            "ALBUM",
            &s(Family::Mono, Weight::Bold, 9.0, t.acc, 0.1),
        );
    }

    // ---------- bottom toolbar (744..800): library · queue · bt · settings --------------------
    // FOUR slots of 120px: library, queue, bluetooth, settings.
    // (`nav::tap` slices the same 120px slots in the same order — keep them in sync.)
    hline(c, 744, t.line);
    let tb = 774.0;
    for (index, slot) in TOOLBAR_CX.iter().enumerate() {
        let cx = *slot as f32;
        match index {
            0 => icons::library(c, cx, tb, 24.0, t.dim),
            1 => icons::queue(c, cx, tb, 24.0, t.dim),
            2 => icons::bt(c, cx, tb, 23.0, t.dim),
            _ => icons::settings(c, cx, tb, 24.0, t.dim),
        }
    }
}

/// Progress rail + transport + toolbar for the idle screen. Geometry is duplicated from `render`
/// deliberately: every one of these is a TAP TARGET that `nav::tap` resolves by coordinate, so the
/// idle screen has to put them in exactly the same places or the controls stop working when
/// nothing is loaded. Only the *state* differs — empty rail, no times, transport shows play.
fn idle_chrome(c: &mut Canvas, t: &Theme, f: &FontSet) {
    fill_rect(c, RAIL_X0, RAIL_Y, RAIL_W, RAIL_H, t.line);

    let ty = 692.0;
    icons::shuffle(c, 44.0, ty, 24.0, t.faint);
    icons::prev(c, 128.0, ty, 38.0, t.faint);
    Circle::with_center(Point::new(240, ty as i32), 92)
        .into_styled(PrimitiveStyle::with_fill(t.acc))
        .draw(c)
        .ok();
    icons::play(c, 240.0, ty, 38.0, t.acc_ink);
    icons::next(c, 352.0, ty, 38.0, t.faint);
    icons::repeat(c, 436.0, ty, 24.0, t.faint);

    hline(c, 744, t.line);
    let tb = 774.0;
    icons::library(c, 60.0, tb, 26.0, t.acc); // the one thing worth tapping from here
    icons::queue(c, 180.0, tb, 26.0, t.dim);
    icons::bt(c, 300.0, tb, 25.0, t.dim);
    icons::settings(c, 420.0, tb, 26.0, t.dim);
    let _ = f;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn np_with(page: u8, viz_size: u8, levels: &[f32]) -> NowPlaying<'_> {
        NowPlaying {
            title: "Atlas Hands",
            artist: "Benjamin Francis Leftwich",
            codec: "FLAC · 24bit / 96.0 kHz",
            badge: "FLAC 24/96",
            clock: "14:32",
            battery: 78,
            elapsed: "1:47",
            remaining: "-2:45",
            progress: 0.39,
            art: "atlas hands",
            art_full: None,
            art_thumb: None,
            liked: false,
            playing: true,
            shuffle: false,
            repeat: 0,
            viz_seed: 2.0,
            viz_kind: 0,
            viz_size,
            viz_levels: Some(levels),
            viz_peaks: None,
            viz_sig: None,
            page,
            scrubbing: false,
            lyrics: false,
        }
    }

    /// How many pixels of the paging block differ between two frames. Counting *accent* pixels
    /// would not work: the translucent sizes blend with the artwork, so none of their pixels is
    /// ever exactly `t.acc`. Differencing against a known-clean frame measures the thing that
    /// actually matters — did anything land on the cover.
    fn art_pixels_differing(a: &Canvas, b: &Canvas) -> usize {
        let mut n = 0;
        for y in PAGE_TOP..PAGE_BOT {
            for x in 0..crate::canvas::W as i32 {
                let i = (y as usize) * crate::canvas::W + x as usize;
                if a.buf[i] != b.buf[i] {
                    n += 1;
                }
            }
        }
        n
    }

    /// A frame of just the cover, with nothing drawn over it — the baseline everything else is
    /// compared against.
    fn bare_cover(t: &Theme, f: &FontSet, levels: &[f32]) -> Canvas {
        let mut c = Canvas::new();
        render(&mut c, t, f, &np_with(0, 0, levels));
        c
    }

    /// "Cover visualiser · OFF" must mean a genuinely untouched cover — nothing drawn over the
    /// artwork at all, not a smaller or relocated visualiser. An earlier design had a BELOW ART
    /// option that satisfied "off the album art" without satisfying "none", and the label has to
    /// keep meaning the stronger thing.
    #[test]
    fn cover_visualiser_off_leaves_the_artwork_completely_clean() {
        let _scale = crate::text::scale_guard();
        let t = Theme::day();
        let f = FontSet::load();
        let levels: Vec<f32> = (0..36).map(|i| 0.2 + 0.7 * (i as f32 / 36.0)).collect();

        // OFF must be pixel-identical to a cover with no visualiser concept at all: render it
        // twice with wildly different spectrum data and require the art block to be the same.
        let clean = bare_cover(&t, &f, &levels);
        let loud: Vec<f32> = vec![1.0; 36];
        let mut clean_loud = Canvas::new();
        render(&mut clean_loud, &t, &f, &np_with(0, 0, &loud));
        assert_eq!(
            art_pixels_differing(&clean, &clean_loud),
            0,
            "OFF let the audio change the cover — something is still being drawn"
        );

        // …and the other sizes really do draw, or the assertion above proves nothing.
        for size in 1..crate::viz::SIZE_COUNT {
            let mut c = Canvas::new();
            render(&mut c, &t, &f, &np_with(0, size, &levels));
            assert!(
                art_pixels_differing(&clean, &c) > 100,
                "size {size} drew nothing at all"
            );
        }
    }

    /// Turning the cover visualiser off must not touch the pages — the spectrum page still draws.
    /// That is the whole reason the row is named "Cover visualiser" rather than "Visualiser".
    #[test]
    fn the_spectrum_page_still_draws_with_the_cover_visualiser_off() {
        // Draws text, so it shares the crate-wide UI-scale lock (see text::scale_guard).
        let _scale = crate::text::scale_guard();
        let t = Theme::day();
        let f = FontSet::load();
        let levels: Vec<f32> = (0..36).map(|i| 0.2 + 0.7 * (i as f32 / 36.0)).collect();
        let clean = bare_cover(&t, &f, &levels);
        let mut c = Canvas::new();
        render(&mut c, &t, &f, &np_with(1, 0, &levels));
        assert!(
            art_pixels_differing(&clean, &c) > 100,
            "the spectrum page went blank because the cover visualiser was off"
        );
    }

    /// THE CONTRACT. Being a style means passing this, in every mode, with and without lyrics.
    /// Assertions on the layout, with no pixels drawn — which is the point of making it pure.
    #[test]
    fn every_style_keeps_the_contract() {
        use crate::style::Style;
        let (w, h) = (crate::canvas::W as i32, crate::canvas::H as i32);
        let header = crate::chrome::HEADER_BOTTOM;
        for style in Style::ALL {
            for night in [false, true] {
                for lyrics in [false, true] {
                    let l = layout(style, night, lyrics);
                    let at = format!("{style:?} night={night} lyrics={lyrics}");
                    // Every control Cinder has, and the Lyrics chip exactly when there are lyrics.
                    let mut want = vec![
                        Hit::Like,
                        Hit::Info,
                        Hit::PlayPause,
                        Hit::Prev,
                        Hit::Next,
                        Hit::Shuffle,
                        Hit::Repeat,
                        Hit::Menu,
                    ];
                    want.extend((0..TOOLBAR_SLOTS as u8).map(Hit::Toolbar));
                    for hit in &want {
                        assert!(l.at(*hit).is_some(), "{at}: no {hit:?}");
                    }
                    assert_eq!(l.at(Hit::Lyrics).is_some(), lyrics, "{at}: the Lyrics chip");
                    assert!(l.regions().len() < MAX_REGIONS, "{at}: targets dropped");
                    for (shape, hit) in l.regions() {
                        let (x, y, bw, bh) = shape.bounds();
                        // On the panel, big enough for a thumb, and reachable at its middle.
                        assert!(
                            x >= 0 && y >= 0 && x + bw <= w + 1 && y + bh <= h + 1,
                            "{at}: {hit:?} off the panel"
                        );
                        assert!(
                            bw >= 44 && bh >= 44,
                            "{at}: {hit:?} is {bw}x{bh}, under 44 px"
                        );
                        let (cx, cy) = shape.centre();
                        assert_eq!(
                            l.hit(cx, cy),
                            Some(*hit),
                            "{at}: {hit:?}'s middle is someone else's"
                        );
                        // The rail's band is the rail's: no control inside it.
                        let clear = y + bh <= l.rail.grab_top || y > l.rail.grab_bot;
                        assert!(
                            *hit == Hit::Menu || clear,
                            "{at}: {hit:?} is inside the rail's grab band"
                        );
                    }
                    let r = l.rail;
                    assert!(
                        r.grab_bot - r.grab_top >= 44,
                        "{at}: the rail's band is under 44 px"
                    );
                    assert!(
                        (r.grab_top..=r.grab_bot).contains(&r.y),
                        "{at}: the rail is outside its own band"
                    );
                    assert!(
                        r.x0 >= 0 && r.x0 + r.w <= w && r.w > 0,
                        "{at}: the rail is off the panel"
                    );
                    assert!(
                        l.page_top < l.page_bot && l.page_bot <= r.grab_top,
                        "{at}: the page block"
                    );
                    if style == Style::Cinder {
                        continue;
                    }
                    // A new style starts clean: no two targets overlap, and the band under the
                    // status bar is the Menu's and the Lyrics chip's alone.
                    assert!(
                        l.page_top >= header,
                        "{at}: the page block reaches into the Menu band"
                    );
                    let own: Vec<&(Shape, Hit)> = l
                        .regions()
                        .iter()
                        .filter(|(_, hit)| !matches!(hit, Hit::Menu | Hit::Lyrics))
                        .collect();
                    for (i, (a, ha)) in own.iter().enumerate() {
                        let (ax, ay, aw, ah) = a.bounds();
                        assert!(ay >= header, "{at}: {ha:?} is in the Menu band");
                        for (b, hb) in &own[i + 1..] {
                            let (bx, by, bw, bh) = b.bounds();
                            let overlap =
                                ax < bx + bw && bx < ax + aw && ay < by + bh && by < ay + ah;
                            assert!(!overlap, "{at}: {ha:?} overlaps {hb:?}");
                        }
                    }
                }
            }
        }
    }

    /// Cinder's layout is today's screen: the taps every existing test makes land where they did.
    #[test]
    fn cinders_layout_answers_as_it_always_has() {
        use crate::style::Style;
        let l = layout(Style::Cinder, false, false);
        for (x, y, want) in [
            (240, 692, Some(Hit::PlayPause)),
            (130, 692, Some(Hit::Prev)),
            (350, 692, Some(Hit::Next)),
            (44, 692, Some(Hit::Shuffle)),
            (436, 692, Some(Hit::Repeat)),
            (HEART_CX, HEART_CY, Some(Hit::Like)),
            (200, 560, Some(Hit::Info)),
            (60, 780, Some(Hit::Toolbar(0))),
            (420, 780, Some(Hit::Toolbar(3))),
            (240, 60, Some(Hit::Menu)),
            (240, 300, None),
        ] {
            assert_eq!(l.hit(x, y), want, "({x}, {y})");
        }
        assert_eq!(
            layout(Style::Cinder, true, false).hit(200, 110),
            Some(Hit::Info),
            "night's title block"
        );
        assert_eq!(
            layout(Style::Cinder, false, true).hit(40, 60),
            Some(Hit::Lyrics),
            "before the Menu band"
        );
    }

    /// Every style draws every state without a panic: day and night, each page, idle, scrubbing,
    /// and every repeat mode, at the smallest and largest UI scale.
    #[test]
    fn every_style_draws_every_state() {
        use crate::style::Style;
        let f = FontSet::load();
        let levels: Vec<f32> = (0..36).map(|i| i as f32 / 36.0).collect();
        for idx in [0, crate::text::SCALE_STEPS.len() - 1] {
            let _scale = crate::text::scale_guard();
            crate::text::set_scale_idx(idx);
            for style in Style::ALL {
                for night in [false, true] {
                    let t = Theme {
                        style,
                        ..if night { Theme::night() } else { Theme::day() }
                    };
                    for page in 0..PAGES {
                        for repeat in 0..=REPEAT_ALBUM {
                            let np = NowPlaying {
                                repeat,
                                scrubbing: repeat == 2,
                                lyrics: page == 1,
                                ..np_with(page, 1, &levels)
                            };
                            let mut c = Canvas::new();
                            render(&mut c, &t, &f, &np);
                        }
                    }
                    let idle = NowPlaying {
                        title: "",
                        artist: "",
                        codec: "",
                        badge: "",
                        ..np_with(0, 0, &levels)
                    };
                    let mut c = Canvas::new();
                    render(&mut c, &t, &f, &idle);
                }
            }
            crate::text::set_scale_idx(crate::text::SCALE_DEFAULT_IDX);
        }
    }
}
