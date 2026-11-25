//! Themes v2 (`docs/PLAN_v0.5.md`): a theme is a folder, `cinder_themes/<id>/theme.conf`, that sets
//! Cinder's look as data instead of Rust: colours (a palette by id, or inline), type, shape,
//! density, and one variant for each kit part. [`DataLook`] is the `style::Look` that draws it,
//! through the same boxes and hit tests every built-in style uses, so a theme cannot move a target.
//!
//! ```text
//! # theme.conf: the palette file's `key = value` syntax
//! name        = Ledger
//! format      = 2
//! palette     = paper        # or day.bg = #…, night.ink = #…: the palette keys, inline
//! type.title  = 30 / 700     # px / weight
//! type.row    = 19 / 600
//! shape.radius = 6           # 0–24
//! shape.cover  = 0           # 0–24
//! density     = big          # compact | big | huge
//! row         = card         # rule | card | plain | mono
//! row.sel     = bar          # wash | bar | caret | ring
//! chip        = fill         # outline | fill | underline | bracket
//! switch      = pill         # square | pill | check
//! play        = ring         # disc | ring | square | text
//! ```
//!
//! Every key is optional and falls back to Cinder, so `themes/cinder/theme.conf` (all of Cinder's
//! values written out) draws exactly what the code does; a test holds it to that. A theme with any
//! problem is refused with every reason, never half-loaded. Layout slots and assets come later
//! (P6); their keys are refused until then rather than silently ignored.

use crate::canvas::{Canvas, W};
use crate::kit::{self, Row, RowDress, CHIP_H, LEFT, RIGHT, SWITCH_H, SWITCH_W};
use crate::palette::Palette;
use crate::text::{self, Family, FontSet, Weight};
use crate::theme::{Theme, Tokens};
use crate::widgets::{fill_rect, fit, stroke_rect, sty};
use embedded_graphics::pixelcolor::Rgb888;

/// The folder themes are read from, next to the settings file.
pub const DIR_NAME: &str = "cinder_themes";
/// The file inside each theme folder.
pub const FILE_NAME: &str = "theme.conf";
/// A theme file is under a kilobyte; anything past this is not one.
pub const MAX_BYTES: u64 = 16 * 1024;
/// The only format this build reads.
pub const FORMAT: u32 = 2;

macro_rules! variants {
    ($(#[$m:meta])* $name:ident { $($v:ident = $s:literal),+ $(,)? }) => {
        $(#[$m])*
        #[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
        pub enum $name { #[default] $($v),+ }
        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$v),+];
            pub fn token(self) -> &'static str { match self { $($name::$v => $s),+ } }
            fn parse(s: &str) -> Option<Self> { Self::ALL.iter().copied().find(|v| v.token() == s) }
            fn words() -> String { Self::ALL.iter().map(|v| v.token()).collect::<Vec<_>>().join(" | ") }
        }
    };
}

variants!(
    /// How much room the UI takes. Big is v0.5's default; Compact is v0.4's normal size.
    Density { Big = "big", Huge = "huge", Compact = "compact" }
);
variants!(
    /// A list row: `rule` a hairline under it, `card` a raised panel, `plain` nothing,
    /// `mono` the title in the mono face.
    RowV { Rule = "rule", Card = "card", Plain = "plain", Mono = "mono" }
);
variants!(
    /// The chosen row's mark.
    SelV { Wash = "wash", Bar = "bar", Caret = "caret", Ring = "ring" }
);
variants!(
    /// A chip.
    ChipV { Outline = "outline", Fill = "fill", Underline = "underline", Bracket = "bracket" }
);
variants!(
    /// A switch.
    SwitchV { Square = "square", Pill = "pill", Check = "check" }
);
variants!(
    /// Now Playing's play button (drawn from P2).
    PlayV { Disc = "disc", Ring = "ring", Square = "square", Text = "text" }
);

/// A loaded theme. `Default` is Cinder.
#[derive(Clone, Debug, PartialEq)]
pub struct DataLook {
    pub id: String,
    pub name: String,
    /// A palette by id (`palette =`), resolved by the app against the palettes it has.
    pub palette: Option<String>,
    /// Inline colours, already through the palette checks.
    pub colours: Option<Tokens>,
    pub title: (f32, Weight),
    pub row_type: (f32, Weight),
    pub radius: i32,
    pub cover_radius: i32,
    pub density: Density,
    pub row: RowV,
    pub sel: SelV,
    pub chip: ChipV,
    pub switch: SwitchV,
    pub play: PlayV,
}

impl Default for DataLook {
    fn default() -> Self {
        DataLook {
            id: "cinder".into(),
            name: "Cinder".into(),
            palette: None,
            colours: None,
            title: (crate::scale::TITLE, Weight::Bold),
            row_type: (crate::scale::ROW, Weight::SemiBold),
            radius: 0,
            cover_radius: 0,
            density: Density::Big,
            row: RowV::Rule,
            sel: SelV::Wash,
            chip: ChipV::Outline,
            switch: SwitchV::Square,
            play: PlayV::Disc,
        }
    }
}

fn weight(s: &str) -> Option<Weight> {
    Some(match s {
        "300" => Weight::Light,
        "400" => Weight::Regular,
        "600" => Weight::SemiBold,
        "700" => Weight::Bold,
        "800" => Weight::ExtraBold,
        _ => return None,
    })
}

/// `19 / 600`: a size in px within `lo..=hi` and a weight.
fn type_role(v: &str, lo: f32, hi: f32) -> Option<(f32, Weight)> {
    let (s, w) = v.split_once('/')?;
    let s: f32 = s.trim().parse().ok()?;
    ((lo..=hi).contains(&s)).then_some(())?;
    Some((s, weight(w.trim())?))
}

impl DataLook {
    /// Parse and check `theme.conf` from the folder `id`. Every problem is reported, as for
    /// palettes: a theme is fixed by copying it back and forth over USB.
    pub fn parse(id: &str, body: &str) -> Result<DataLook, Vec<String>> {
        let mut d = DataLook {
            id: id.to_string(),
            name: id.to_string(),
            ..Default::default()
        };
        let mut errs = Vec::new();
        if !crate::palette::valid_id(id) {
            errs.push(format!(
                "`{id}` cannot be a theme folder name: use lowercase letters, digits, - and _"
            ));
        }
        let mut colours = String::new();
        let mut seen: Vec<&str> = Vec::new();
        let mut format = None;
        for (i, raw) in body.lines().enumerate() {
            let n = i + 1;
            // A comment is a line starting `#`, or ` # ` after a value (a colour is `#1a2b3c`).
            let line = raw.trim();
            let line = if line.starts_with('#') {
                ""
            } else {
                line.split(" # ").next().unwrap_or("").trim()
            };
            if line.is_empty() {
                continue;
            }
            let Some((k, v)) = line.split_once('=') else {
                errs.push(format!("line {n}: expected `key = value`"));
                continue;
            };
            let (k, v) = (k.trim(), v.trim());
            if seen.contains(&k) {
                errs.push(format!("line {n}: `{k}` is set twice"));
                continue;
            }
            seen.push(k);
            let bad = |want: &str| format!("line {n}: `{k}` wants {want}, got `{v}`");
            macro_rules! pick {
                ($field:expr, $ty:ty) => {
                    match <$ty>::parse(v) {
                        Some(x) => $field = x,
                        None => errs.push(bad(&<$ty>::words())),
                    }
                };
            }
            match k {
                "name" => {
                    let len = v.chars().count();
                    if len == 0 || len > crate::palette::MAX_NAME || v.chars().any(char::is_control)
                    {
                        errs.push(bad(&format!(
                            "1 to {} characters",
                            crate::palette::MAX_NAME
                        )));
                    } else {
                        d.name = v.to_string();
                    }
                }
                "format" => format = Some(v.to_string()),
                "palette" => d.palette = Some(v.to_string()),
                "type.title" => match type_role(v, 18.0, 40.0) {
                    Some(r) => d.title = r,
                    None => errs.push(bad("a size from 18 to 40 and a weight: `30 / 700`")),
                },
                "type.row" => match type_role(v, 16.0, 28.0) {
                    Some(r) => d.row_type = r,
                    None => errs.push(bad("a size from 16 to 28 and a weight: `19 / 600`")),
                },
                "shape.radius" | "shape.cover" => match v.parse::<i32>() {
                    Ok(r) if (0..=24).contains(&r) => {
                        *if k == "shape.radius" {
                            &mut d.radius
                        } else {
                            &mut d.cover_radius
                        } = r
                    }
                    _ => errs.push(bad("a whole number from 0 to 24")),
                },
                "density" => pick!(d.density, Density),
                "row" => pick!(d.row, RowV),
                "row.sel" => pick!(d.sel, SelV),
                "chip" => pick!(d.chip, ChipV),
                "switch" => pick!(d.switch, SwitchV),
                "play" => pick!(d.play, PlayV),
                _ if k.starts_with("day.") || k.starts_with("night.") => {
                    colours.push_str(line);
                    colours.push('\n');
                }
                _ if k.starts_with("layout.")
                    || k.starts_with("asset.")
                    || k == "icons"
                    || k == "type.sans" =>
                {
                    errs.push(format!(
                        "line {n}: `{k}` is not supported by this version of Cinder yet"
                    ))
                }
                _ => errs.push(format!("line {n}: unknown key `{k}`")),
            }
        }
        match format.as_deref() {
            Some(f) if f == FORMAT.to_string() => {}
            Some(f) => errs.push(format!("`format = {f}`: this Cinder reads format {FORMAT}")),
            None => errs.push(format!("`format = {FORMAT}` is missing")),
        }
        if !colours.is_empty() {
            if d.palette.is_some() {
                errs.push("set `palette` or inline colours, not both".into());
            }
            // The palette parser and its readability rules, unchanged. Its id is never shown.
            match Palette::parse("theme", &colours) {
                Ok(p) => d.colours = Some(p.tokens),
                Err(e) => errs.extend(e.into_iter().map(|e| format!("colours: {e}"))),
            }
        }
        if errs.is_empty() {
            Ok(d)
        } else {
            Err(errs)
        }
    }
}

// ── The active theme ────────────────────────────────────────────────────────────────────────

static ACTIVE: std::sync::atomic::AtomicPtr<DataLook> =
    std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());

/// Make `d` the look `Style::Data` draws with.
// ponytail: each switch leaks one DataLook (~200 bytes) so the fn-pointer dress can read it without
// a lock; fine for a theme picked by hand, an arena if themes ever change per frame.
pub fn set_active(d: DataLook) {
    let p = Box::into_raw(Box::new(d));
    ACTIVE.store(p, std::sync::atomic::Ordering::Release);
}

/// The active theme, Cinder's when none was set.
pub fn active() -> &'static DataLook {
    static CINDER: std::sync::OnceLock<DataLook> = std::sync::OnceLock::new();
    let p = ACTIVE.load(std::sync::atomic::Ordering::Acquire);
    if p.is_null() {
        CINDER.get_or_init(DataLook::default)
    } else {
        // SAFETY: only `set_active` stores, from a leaked Box that is never freed.
        unsafe { &*p }
    }
}

// ── Shapes ──────────────────────────────────────────────────────────────────────────────────

/// Inset of row `dy` (0 at the top) of a `h`-tall box with corner radius `r`.
fn corner_inset(dy: i32, h: i32, r: i32) -> i32 {
    let d = if dy < r {
        r - dy
    } else if dy >= h - r {
        dy - (h - r) + 1
    } else {
        0
    };
    if d == 0 {
        return 0;
    }
    let rf = r as f32;
    let k = d as f32 - 0.5;
    (rf - (rf * rf - k * k).max(0.0).sqrt()).round() as i32
}

/// A filled box with round corners; square at `r == 0`, exactly `fill_rect`.
pub fn fill_round(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, r: i32, col: Rgb888) {
    let r = r.min(w / 2).min(h / 2);
    if r <= 0 {
        return fill_rect(c, x, y, w, h, col);
    }
    for dy in 0..h {
        let i = corner_inset(dy, h, r);
        fill_rect(c, x + i, y + dy, w - 2 * i, 1, col);
    }
}

/// A `weight`-px outline with round corners; square at `r == 0`, exactly `stroke_rect`.
pub fn stroke_round(
    c: &mut Canvas,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    r: i32,
    col: Rgb888,
    weight: i32,
) {
    let r = r.min(w / 2).min(h / 2);
    if r <= 0 {
        return stroke_rect(c, x, y, w, h, col, weight as u32);
    }
    // Centred on the edge like `stroke_rect`: a 2 px ring sits 1 px outside the box.
    let o = weight / 2;
    let (x, y, w, h, r) = (x - o, y - o, w + 2 * o, h + 2 * o, r + o);
    let (ir, ih) = ((r - weight).max(0), h - 2 * weight);
    for dy in 0..h {
        let o = corner_inset(dy, h, r);
        let idy = dy - weight;
        if idy < 0 || idy >= ih {
            fill_rect(c, x + o, y + dy, w - 2 * o, 1, col);
            continue;
        }
        let i = weight + if ir > 0 { corner_inset(idy, ih, ir) } else { 0 };
        fill_rect(c, x + o, y + dy, (i - o).max(1), 1, col);
        fill_rect(c, x + w - i, y + dy, (i - o).max(1), 1, col);
    }
}

// ── The dress ───────────────────────────────────────────────────────────────────────────────
// RowDress takes plain fns, so each reads the active theme for its numbers.

fn title_sans(t: &Theme, sel: bool) -> text::TextStyle {
    let (s, w) = active().row_type;
    sty(Family::Sans, w, s, if sel { t.acc } else { t.ink }, 0.0)
}
fn title_mono(t: &Theme, sel: bool) -> text::TextStyle {
    let (s, _) = active().row_type;
    sty(
        Family::Mono,
        Weight::Regular,
        s - 2.0,
        if sel { t.acc } else { t.ink },
        0.04,
    )
}
fn sel_bar(c: &mut Canvas, t: &Theme, _: &FontSet, y: i32, h: i32) {
    fill_rect(c, 0, y, 4, h, t.acc)
}
fn sel_caret(c: &mut Canvas, t: &Theme, f: &FontSet, y: i32, h: i32) {
    let st = sty(Family::Mono, Weight::Bold, 16.0, t.acc, 0.0);
    text::draw(c, f, 4.0, (y + h / 2 + 6) as f32, ">", &st);
}
fn sel_ring(c: &mut Canvas, t: &Theme, _: &FontSet, y: i32, h: i32) {
    stroke_round(
        c,
        LEFT - 12,
        y + 2,
        RIGHT - LEFT + 24,
        h - 4,
        active().radius,
        t.acc,
        2,
    )
}
fn no_rule(_: &mut Canvas, _: &Theme, _: i32) {}

impl crate::style::Look for DataLook {
    fn name(&self) -> &'static str {
        // Settings names themes from the folder list, not through Look.
        "Theme"
    }
    fn token(&self) -> &'static str {
        "theme"
    }
    fn about(&self) -> &'static str {
        ""
    }

    fn row(&self, c: &mut Canvas, t: &Theme, f: &FontSet, y: i32, h: i32, r: &Row) -> i32 {
        let base = &kit::CINDER_ROW;
        if self.row == RowV::Card {
            fill_round(
                c,
                LEFT - 12,
                y + 3,
                RIGHT - LEFT + 24,
                h - 6,
                self.radius,
                t.panel,
            );
        }
        let d = RowDress {
            title: if self.row == RowV::Mono {
                title_mono
            } else {
                title_sans
            },
            sub: base.sub,
            value: base.value,
            sel: match self.sel {
                SelV::Wash => base.sel,
                SelV::Bar => sel_bar,
                SelV::Caret => sel_caret,
                SelV::Ring => sel_ring,
            },
            rule: if self.row == RowV::Rule {
                base.rule
            } else {
                no_rule
            },
            open: base.open,
        };
        kit::row_dressed(c, t, f, y, h, r, &d)
    }

    fn switch(&self, c: &mut Canvas, t: &Theme, x: i32, y: i32, on: bool) {
        match self.switch {
            SwitchV::Square => kit::cinder_switch(c, t, x, y, on),
            SwitchV::Pill => {
                let col = if on { t.acc } else { t.ctrl() };
                if on {
                    fill_round(c, x, y, SWITCH_W, SWITCH_H, SWITCH_H / 2, col);
                } else {
                    stroke_round(c, x, y, SWITCH_W, SWITCH_H, SWITCH_H / 2, col, 1);
                }
                let k = 16;
                let kx = if on { x + SWITCH_W - 3 - k } else { x + 3 };
                fill_round(
                    c,
                    kx,
                    y + 3,
                    k,
                    k,
                    k / 2,
                    if on { t.acc_ink } else { t.dim },
                );
            }
            SwitchV::Check => {
                let s = SWITCH_H;
                let bx = x + SWITCH_W - s;
                if on {
                    fill_round(c, bx, y, s, s, self.radius.min(4), t.acc);
                    fill_rect(c, bx + 7, y + 7, s - 14, s - 14, t.acc_ink);
                } else {
                    stroke_round(c, bx, y, s, s, self.radius.min(4), t.ctrl(), 1);
                }
            }
        }
    }

    fn chips(
        &self,
        c: &mut Canvas,
        t: &Theme,
        f: &FontSet,
        y: i32,
        labels: &[&str],
        sel: Option<usize>,
        cols: usize,
    ) -> i32 {
        if self.chip == ChipV::Outline && self.radius == 0 {
            return kit::cinder_chips(c, t, f, y, labels, sel, cols);
        }
        let mono = self.chip == ChipV::Bracket;
        let fam = if mono { Family::Mono } else { Family::Sans };
        let size = kit::chip_text_size(
            f,
            labels,
            cols,
            fam,
            Weight::SemiBold,
            if mono { 32 } else { 16 },
        );
        let r = self.radius;
        for (i, l) in labels.iter().enumerate() {
            let l = crate::i18n::text(l);
            let (x, w) = kit::chip_span(i, cols);
            let on = sel == Some(i);
            let mut ink = t.ink;
            match self.chip {
                ChipV::Outline => {
                    if on {
                        fill_round(c, x, y, w, CHIP_H, r, t.row_sel);
                        stroke_round(c, x, y, w, CHIP_H, r, t.acc, 2);
                    } else {
                        stroke_round(c, x, y, w, CHIP_H, r, t.ctrl(), 1);
                    }
                }
                ChipV::Fill => {
                    fill_round(c, x, y, w, CHIP_H, r, if on { t.acc } else { t.panel });
                    if on {
                        ink = t.acc_ink;
                    }
                }
                ChipV::Underline => {
                    fill_rect(
                        c,
                        x,
                        y + CHIP_H - if on { 3 } else { 1 },
                        w,
                        if on { 3 } else { 1 },
                        if on { t.acc } else { t.line },
                    );
                    if !on {
                        ink = t.dim;
                    }
                }
                ChipV::Bracket => {
                    if !on {
                        ink = t.dim;
                    }
                }
            }
            let st = sty(fam, Weight::SemiBold, size, ink, 0.0);
            let s = fit(f, l, &st, (w - if mono { 32 } else { 16 }) as f32);
            let tw = text::measure(f, &s, &st);
            let tx = x as f32 + (w as f32 - tw) / 2.0;
            let base = (y + CHIP_H / 2 + 6) as f32;
            text::draw(c, f, tx, base, &s, &st);
            if mono && on {
                let b = sty(fam, Weight::SemiBold, size, t.acc, 0.0);
                text::draw(c, f, tx - text::measure(f, "[", &b) - 2.0, base, "[", &b);
                text::draw(c, f, tx + tw + 2.0, base, "]", &b);
            }
        }
        y + CHIP_H
    }

    fn primary_button(&self, c: &mut Canvas, t: &Theme, f: &FontSet, y: i32, label: &str) -> i32 {
        if self.radius == 0 {
            return kit::cinder_primary_button(c, t, f, y, label);
        }
        fill_round(c, LEFT, y, RIGHT - LEFT, kit::BUTTON_H, self.radius, t.acc);
        let st = sty(
            Family::Sans,
            Weight::Bold,
            crate::scale::ROW,
            t.acc_ink,
            0.0,
        );
        let s = fit(f, label, &st, (RIGHT - LEFT - 24) as f32);
        let tw = text::measure(f, &s, &st);
        text::draw(
            c,
            f,
            (W as f32 - tw) / 2.0,
            (y + kit::BUTTON_H / 2 + 7) as f32,
            &s,
            &st,
        );
        y + kit::BUTTON_H
    }
}

/// The kit parts a theme dresses, one of each state, the way screens draw them: the `--v5`
/// preview and the tests' reference sheet.
pub fn kit_preview(t: &Theme, f: &FontSet) -> Canvas {
    use crate::kit::Trail;
    let mut c = Canvas::new();
    c.fill(t.bg);
    let mut y = kit::section_label(&mut c, t, f, 56, "Playback", None);
    for r in [
        Row::new("Sleep timer").trail(Trail::Value("30 MIN")),
        Row::new("Atlas Hands")
            .sub("Benjamin Francis Leftwich")
            .sel(true),
        Row::new("Bluetooth").trail(Trail::Switch(true)),
        Row::new("Gapless").trail(Trail::Switch(false)),
        Row::new("Palette").trail(Trail::Open("CINDER")),
        Row::new("Home screen").trail(Trail::Tag("HOME")),
    ] {
        y = kit::row(&mut c, t, f, y, kit::ROW_H(), &r);
    }
    y = kit::section_label(&mut c, t, f, y + 8, "Sleep after", None);
    y = kit::chips(&mut c, t, f, y, &["15", "30", "45", "60"], Some(1));
    kit::primary_button(&mut c, t, f, y + 24, "Pair new device");
    c
}

/// Cinder's own theme, as the file that ships.
pub const CINDER_CONF: &str = include_str!("../themes/cinder/theme.conf");

#[cfg(test)]
mod tests;
