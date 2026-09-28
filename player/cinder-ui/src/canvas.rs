//! A 480x800 XRGB8888 software canvas. Stores `u32` pixels as `0x00RRGGBB`,
//! matching the device framebuffer exactly so the device backend is a memcpy.
//! Implements `embedded-graphics` `DrawTarget` so primitives draw onto it, and
//! exposes `blend()` for the fontdue text path.

use embedded_graphics::pixelcolor::Rgb888;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::Rectangle;

pub const W: usize = 480;
pub const H: usize = 800;

pub struct Canvas {
    pub buf: Vec<u32>, // W*H, 0x00RRGGBB
    /// Vertical clip band [clip_top, clip_bot) enforced by every pixel write. Lists set this
    /// around their scroll area so pixel-offset (partially visible) rows can't paint over the
    /// chrome above or below; everything else draws with the full-screen default.
    clip_top: i32,
    clip_bot: i32,
    /// Horizontal clip band [clip_x0, clip_x1), the exact counterpart of the vertical one. The
    /// marquee sets it around a title's box so the scrolled run is cut off at the box edge instead
    /// of sliding across whatever is beside it.
    ///
    /// Like the vertical band, a rejection here is NOT counted as overflow: `note_oob` only counts
    /// pixels that fall off the PANEL, so narrowing the band suppresses nothing a layout audit
    /// wants to see. That is the whole reason this is a band rather than the callers pre-trimming
    /// the string — a partially visible glyph has no substring.
    clip_x0: i32,
    clip_x1: i32,
    /// Horizontal translation applied to EVERY pixel write, in px. Zero for normal drawing; set
    /// while a swiped list row is drawn so the whole row — text, separators, cover art, icons —
    /// moves as one piece. Doing it here rather than threading an offset through each draw call
    /// is what makes "the whole bar follows the finger" a two-line change at the call site
    /// instead of an edit to every primitive in the row.
    off_x: i32,
    /// Pixels a draw call asked for that fell OFF THE PANEL — x outside 0..W, or y outside 0..H.
    ///
    /// Every write here is silently clipped, which is the right runtime behaviour (a stray glyph
    /// must never scribble outside the framebuffer) but hides layout bugs completely: text that
    /// runs past the right margin, a row drawn below the panel, a value that overflows its pill —
    /// all of them just disappear, and the screenshot looks merely "a bit tight" instead of wrong.
    /// Counting them turns "does anything clip?" into an assertion. See `tests/ui_overflow.rs`.
    ///
    /// Rejections caused by the CLIP BAND are deliberately NOT counted: lists set a band precisely
    /// so half-scrolled rows are cut off, and that is correct behaviour rather than a defect.
    ///
    /// Split by AXIS, because the two mean opposite things. Running off the LEFT or RIGHT edge is
    /// always a layout defect — nothing on this device scrolls horizontally, so a pixel past the
    /// margin is content the user can never see. Running off the TOP or BOTTOM is usually a
    /// scrolling list drawing rows outside the viewport, which is ordinary; Settings alone is
    /// 919px of content on an 800px panel.
    oob_x: u32,
    oob_y: u32,
    /// The text-collision audit, OFF (`None`) except in tests that ask for it with
    /// [`Canvas::track_text`]. See [`TextInk`].
    text_ink: Option<Box<TextInk>>,
}

/// Which text run put solid ink on each pixel, so a test can see two runs landing on each other.
///
/// The off-panel count (`oob_x`) only sees text that leaves the PANEL. Text that stays on the panel
/// but runs into its neighbour — an artist name into the codec beside it, a row value into the
/// row's title — is just as unreadable and was invisible to every gate. With this on, `text::draw`
/// tags each call as a run and records the pixels it inks; a pixel inked by one run and then by a
/// different one is a collision. Any non-text write clears the pixel's owner, because a fill or an
/// icon drawn over text hides it, and the text drawn on top of THAT is not colliding with anything
/// the user can see.
///
/// Only solid coverage counts (see [`TEXT_INK_ALPHA`]): two runs whose anti-aliased fringes touch
/// are adjacent, not overlapping.
#[derive(Default)]
pub struct TextInk {
    owner: Vec<u16>,
    runs: Vec<String>,
    cur: u16,
    hits: std::collections::BTreeMap<(u16, u16), u32>,
    /// Ink of each run that something drawn LATER covered: a fill, an icon, a swatch.
    hidden: std::collections::BTreeMap<u16, u32>,
}

/// Coverage at or above which a glyph pixel counts as ink for the collision audit.
pub const TEXT_INK_ALPHA: u8 = 160;

static AUDIT_NEW: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Make every `Canvas` created from now on start with the text-collision audit on. For harnesses
/// that create their canvases deep inside code they do not own (`cinder-host --audit`); a test with
/// its own canvas calls [`Canvas::track_text`] instead. Off by default, and nothing on the device
/// turns it on.
pub fn audit_new_canvases(on: bool) {
    AUDIT_NEW.store(on, std::sync::atomic::Ordering::Relaxed);
}

impl Default for Canvas {
    fn default() -> Self {
        Self::new()
    }
}

impl Canvas {
    pub fn new() -> Self {
        Self {
            buf: vec![0; W * H],
            clip_top: 0,
            clip_bot: H as i32,
            clip_x0: 0,
            clip_x1: W as i32,
            off_x: 0,
            oob_x: 0,
            oob_y: 0,
            text_ink: AUDIT_NEW
                .load(std::sync::atomic::Ordering::Relaxed)
                .then(|| Box::new(TextInk { owner: vec![0; W * H], ..TextInk::default() })),
        }
    }

    /// Translate every subsequent draw horizontally by `dx` px. Pair with `clear_offset_x`.
    /// The vertical clip band is NOT affected — a shifted row still can't paint outside the list.
    pub fn set_offset_x(&mut self, dx: i32) {
        self.off_x = dx;
    }

    pub fn clear_offset_x(&mut self) {
        self.off_x = 0;
    }

    /// Restrict drawing to rows `top..bottom` (screen coords). Pair with `clear_clip`.
    pub fn set_clip_y(&mut self, top: i32, bottom: i32) {
        self.clip_top = top.clamp(0, H as i32);
        self.clip_bot = bottom.clamp(self.clip_top, H as i32);
    }

    pub fn clear_clip(&mut self) {
        self.clip_top = 0;
        self.clip_bot = H as i32;
    }

    /// Restrict drawing to columns `x0..x1` (screen coords, BEFORE `off_x`). Pair with
    /// `clear_clip_x`. Used by the marquee; everything else draws with the full-width default.
    pub fn set_clip_x(&mut self, x0: i32, x1: i32) {
        self.clip_x0 = x0.clamp(0, W as i32);
        self.clip_x1 = x1.clamp(self.clip_x0, W as i32);
    }
    pub fn clear_clip_x(&mut self) {
        self.clip_x0 = 0;
        self.clip_x1 = W as i32;
    }

    pub fn fill(&mut self, c: Rgb888) {
        self.buf.fill(to_u32(c));
        if let Some(ink) = self.text_ink.as_deref_mut() {
            ink.owner.fill(0);
        }
    }

    /// Turn on the text-collision audit ([`TextInk`]), clearing anything it had recorded.
    pub fn track_text(&mut self) {
        self.text_ink = Some(Box::new(TextInk { owner: vec![0; W * H], ..TextInk::default() }));
    }

    /// Is the collision audit on? `text::draw` asks once per call, so the per-pixel cost at
    /// runtime is nothing.
    #[inline]
    pub fn tracking_text(&self) -> bool {
        self.text_ink.is_some()
    }

    /// Start a new text run for the audit. `label` names it in a collision report.
    ///
    /// Text drawn under a swipe offset is not tracked: the row is following the finger, and what it
    /// slides under or into on the way is the gesture, not the layout (the same exemption the
    /// off-panel count gives it).
    pub fn text_run(&mut self, label: &str) {
        let moving = self.off_x != 0;
        if let Some(ink) = self.text_ink.as_deref_mut() {
            if moving {
                ink.cur = 0;
                return;
            }
            ink.runs.push(label.to_string());
            ink.cur = ink.runs.len().min(u16::MAX as usize) as u16;
        }
    }

    /// A modal sheet or overlay is about to be drawn over the screen. The text under it is covered
    /// on purpose, so the audit forgets who owns those pixels instead of reporting them hidden.
    pub fn begin_layer(&mut self) {
        if let Some(ink) = self.text_ink.as_deref_mut() {
            ink.owner.fill(0);
        }
    }

    /// Every pair of text runs that inked the same pixels since `track_text`, with the count.
    pub fn text_collisions(&self) -> Vec<(String, String, u32)> {
        let Some(ink) = self.text_ink.as_deref() else { return Vec::new() };
        let name = |i: u16| ink.runs.get(i as usize - 1).cloned().unwrap_or_default();
        ink.hits.iter().map(|(&(a, b), &n)| (name(a), name(b), n)).collect()
    }

    /// Every text run whose ink something drawn later covered, with how many pixels.
    pub fn text_hidden(&self) -> Vec<(String, u32)> {
        let Some(ink) = self.text_ink.as_deref() else { return Vec::new() };
        let name = |i: u16| ink.runs.get(i as usize - 1).cloned().unwrap_or_default();
        ink.hidden.iter().map(|(&r, &n)| (name(r), n)).collect()
    }

    /// Clear the owner of pixel `idx`: something that is not text was drawn over it.
    #[inline]
    fn unink(&mut self, idx: usize) {
        if let Some(ink) = self.text_ink.as_deref_mut() {
            let prev = std::mem::take(&mut ink.owner[idx]);
            if prev != 0 {
                *ink.hidden.entry(prev).or_insert(0) += 1;
            }
        }
    }

    fn unink_span(&mut self, a: usize, b: usize) {
        if let Some(ink) = self.text_ink.as_deref_mut() {
            for o in &mut ink.owner[a..b] {
                let prev = std::mem::take(o);
                if prev != 0 {
                    *ink.hidden.entry(prev).or_insert(0) += 1;
                }
            }
        }
    }

    /// Pixels asked for past the LEFT or RIGHT margin since `reset_oob` — always a layout defect.
    pub fn oob_x(&self) -> u32 {
        self.oob_x
    }

    /// Pixels asked for above or below the panel — normal for a scrolling list.
    pub fn oob_y(&self) -> u32 {
        self.oob_y
    }

    pub fn reset_oob(&mut self) {
        self.oob_x = 0;
        self.oob_y = 0;
    }

    /// Is the horizontal clip band NARROWED from the full panel?
    ///
    /// This is what separates "the marquee is drawing the rest of a title outside its box, as
    /// designed" from "a layout ran off the right margin". A narrowed band means the caller has
    /// taken explicit responsibility for everything outside it, so rejections there are not
    /// counted as overflow — without this the audit reported 16,813 px of overflow for a title
    /// that is behaving exactly as intended. At the default band every rejection is a real defect
    /// and is counted as before.
    #[inline]
    fn x_band_active(&self) -> bool {
        self.clip_x0 > 0 || self.clip_x1 < W as i32
    }

    /// Record one rejected pixel, by axis. Horizontal wins when both are out: a pixel that is off
    /// to the right AND below is reported as the horizontal defect, which is the actionable one.
    ///
    /// A row drawn under a swipe offset (`set_offset_x`) is exempt for the same reason as the
    /// band: the whole row is following the finger, and the part that leaves the glass is the
    /// gesture, not the layout.
    #[inline]
    fn note_oob(&mut self, x: i32, y: i32) {
        if self.x_band_active() || self.off_x != 0 {
            return;
        }
        if x < 0 || x >= W as i32 {
            self.oob_x = self.oob_x.saturating_add(1);
        } else if y < 0 || y >= H as i32 {
            self.oob_y = self.oob_y.saturating_add(1);
        }
    }

    #[inline]
    pub fn put(&mut self, x: i32, y: i32, v: u32) {
        let x = x + self.off_x;
        if x >= 0
            && (x as usize) < W
            && x >= self.clip_x0
            && x < self.clip_x1
            && y >= self.clip_top
            && y < self.clip_bot
        {
            let idx = y as usize * W + x as usize;
            self.buf[idx] = v;
            self.unink(idx);
            return;
        }
        self.note_oob(x, y);
    }

    /// Alpha-blend `c` over the existing pixel with coverage `a` (0..=255).
    #[inline]
    pub fn blend(&mut self, x: i32, y: i32, c: Rgb888, a: u8) {
        if let Some(idx) = self.blend_px(x, y, c, a) {
            self.unink(idx);
        }
    }

    /// `blend` for a glyph pixel of the current text run: the same write, and with the audit on it
    /// records who inked the pixel and whether another run had it already.
    #[inline]
    pub fn blend_text(&mut self, x: i32, y: i32, c: Rgb888, a: u8) {
        let Some(idx) = self.blend_px(x, y, c, a) else { return };
        if a < TEXT_INK_ALPHA {
            return;
        }
        if let Some(ink) = self.text_ink.as_deref_mut().filter(|i| i.cur != 0) {
            let prev = ink.owner[idx];
            if prev != 0 && prev != ink.cur {
                *ink.hits.entry((prev, ink.cur)).or_insert(0) += 1;
            }
            ink.owner[idx] = ink.cur;
        }
    }

    /// The blend itself. Returns the pixel's index when it was written.
    #[inline]
    fn blend_px(&mut self, x: i32, y: i32, c: Rgb888, a: u8) -> Option<usize> {
        let x = x + self.off_x;
        if x < 0
            || x as usize >= W
            || x < self.clip_x0
            || x >= self.clip_x1
            || y < self.clip_top
            || y >= self.clip_bot
        {
            // A zero-coverage blend is a no-op the rasteriser emits for glyph edges; counting it
            // would report overflow for text that merely ENDS at the margin.
            if a > 0 {
                self.note_oob(x, y);
            }
            return None;
        }
        let idx = y as usize * W + x as usize;
        let dst = self.buf[idx];
        let (a, ia) = (a as u32, 255 - a as u32);
        let dr = (dst >> 16) & 0xff;
        let dg = (dst >> 8) & 0xff;
        let db = dst & 0xff;
        let r = div255(dr * ia + c.r() as u32 * a);
        let g = div255(dg * ia + c.g() as u32 * a);
        let b = div255(db * ia + c.b() as u32 * a);
        self.buf[idx] = (r << 16) | (g << 8) | b;
        Some(idx)
    }

    /// A writable, clipped run of ONE row: the destination slice, plus how many leading source
    /// pixels fell off the left edge so the caller can advance its own pointer.
    ///
    /// This exists so blitters can pay the clip test once per row instead of once per pixel.
    /// `put` is correct but it re-checks four bounds and recomputes an index for every pixel, and
    /// a full-bleed 480x480 cover is 230,400 of them — measured at ~1 ms/frame on the host, which
    /// is most of what a Now Playing frame costs, redrawn 20x a second while the visualiser runs.
    pub fn row_run(&mut self, y: i32, x: i32, len: usize) -> Option<(usize, &mut [u32])> {
        let x = x + self.off_x;
        if y < self.clip_top || y >= self.clip_bot || len == 0 {
            return None;
        }
        let x1 = x + len as i32;
        let over_x = (-x).max(0) + (x1 - W as i32).max(0);
        if over_x > 0 && !self.x_band_active() && self.off_x == 0 {
            self.oob_x = self.oob_x.saturating_add(over_x as u32);
        }
        if y < 0 || y >= H as i32 {
            self.oob_y = self.oob_y.saturating_add((len as i32 - over_x).max(0) as u32);
        }
        if x1 <= 0 || x >= W as i32 {
            return None;
        }
        // The horizontal band narrows the RUN, and `skip` grows with it so the caller's source
        // pointer still lines up with the first pixel actually written.
        let lo = x.max(0).max(self.clip_x0);
        let hi = x1.min(W as i32).min(self.clip_x1);
        if hi <= lo {
            return None;
        }
        let skip = (lo - x) as usize;
        let dx0 = lo as usize;
        let dx1 = hi as usize;
        if dx1 <= dx0 {
            return None;
        }
        let row = y as usize * W;
        self.unink_span(row + dx0, row + dx1);
        Some((skip, &mut self.buf[row + dx0..row + dx1]))
    }

    /// RGB byte triples for PNG export (host backend).
    pub fn to_rgb_bytes(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(W * H * 3);
        for &p in &self.buf {
            v.push((p >> 16) as u8);
            v.push((p >> 8) as u8);
            v.push(p as u8);
        }
        v
    }
}

/// Rounded `x / 255` without a division, exact for the 0..=65535 range alpha blending produces.
///
/// This matters far more here than it looks: the device is an ARMv7-A core with no hardware
/// integer divide, so `/ 255` compiles to a `__aeabi_uidiv` CALL. `blend` runs once per glyph
/// pixel — order 10^5 times a frame while a text list scrolls — and was paying three of them.
#[inline]
fn div255(x: u32) -> u32 {
    let t = x + 128;
    (t + (t >> 8)) >> 8
}

#[inline]
pub fn to_u32(c: Rgb888) -> u32 {
    ((c.r() as u32) << 16) | ((c.g() as u32) << 8) | c.b() as u32
}

impl OriginDimensions for Canvas {
    fn size(&self) -> Size {
        Size::new(W as u32, H as u32)
    }
}

impl DrawTarget for Canvas {
    type Color = Rgb888;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(p, c) in pixels {
            self.put(p.x, p.y, to_u32(c));
        }
        Ok(())
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        let v = to_u32(color);
        let x0 = (area.top_left.x + self.off_x).max(0);
        let y0 = area.top_left.y.max(self.clip_top);
        let x1 = (area.top_left.x + self.off_x + area.size.width as i32).min(W as i32);
        let y1 = (area.top_left.y + area.size.height as i32).min(self.clip_bot);
        // Row-at-a-time slice fill, not pixel-at-a-time: this is the single hottest primitive in
        // the UI (every row background, separator, band and panel goes through it), and the
        // per-pixel form paid an index calculation and a bounds check for each of ~400k pixels a
        // frame. `[T]::fill` on a slice compiles to a memset the pixel loop can't become.
        if x1 > x0 {
            for y in y0..y1 {
                let row = y as usize * W;
                self.buf[row + x0 as usize..row + x1 as usize].fill(v);
                self.unink_span(row + x0 as usize, row + x1 as usize);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::div255;

    /// The shift form must agree with real division across everything blending can produce,
    /// otherwise text picks up a colour cast that no test would otherwise catch.
    #[test]
    fn div255_matches_division() {
        for x in 0..=65535u32 {
            assert_eq!(div255(x), (x + 127) / 255, "x={x}");
        }
    }
}
