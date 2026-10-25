//! The type scale and the row heights — one place, so a row is the same object on every screen.
//!
//! WHY THIS FILE EXISTS. The 2026-09-06 UI audit counted **24 font sizes** across 56
//! (family, weight, size) combinations, most of them one pixel apart, and **eleven row heights**
//! with a *track row* — the same object, with the same meaning — drawn at three of them. Six
//! different treatments were in use for "the name of the thing on this row". None of that was a
//! decision; it was what happens when each screen picks its own numbers.
//!
//! The audit deliberately left it open, because collapsing the sizes reflows every screen and
//! invalidates the committed previews, and that is a design call rather than an audit edit. The
//! call was made on 2026-09-12: **four body sizes, three weights, and one track row height.**
//!
//! HOW TO USE IT. A screen never writes a size or a row height as a literal again — it names the
//! ROLE. If a new role genuinely does not exist here, add it here with a sentence saying what it
//! is for, so the next screen finds it instead of inventing 21.0.
//!
//! What is deliberately NOT on the scale: the two hero one-offs (the 76 px lock-screen clock and
//! the 86/88 px FM dial), which are single-purpose display type and are not "body text one step
//! larger"; and the mono micro-labels at 10-12 px, which are a separate open item (see the
//! migration note at the bottom).

// ── the type scale ─────────────────────────────────────────────────────────────────────────

/// Screen titles and the large values a screen exists to show.
pub const TITLE: f32 = 22.0;

/// **The name of the thing on this row.** Every list, every screen: SemiBold for a row you act
/// on, Regular where the row is a file name or a field rather than a title.
pub const ROW: f32 = 19.0;

/// The value beside a label, the second line under a row title, a unit — anything that is read
/// after the row label rather than instead of it.
pub const SECONDARY: f32 = 16.0;

/// Captions, hints, and the uppercase eyebrows over a group of rows.
pub const CAPTION: f32 = 13.0;

// ── row heights ────────────────────────────────────────────────────────────────────────────

// ── Big view ───────────────────────────────────────────────────────────────────────────────
//
// Settings ▸ Big view (off by default). The four row heights below are FUNCTIONS of it, so
// every list on every screen grows together, and render and hit test — which both read these —
// cannot disagree. Off, every number is exactly what it was as a constant, which the golden
// previews prove: a pixel that moves with Big view off is a bug in this refactor.
//
// Reported 2026-10-25 (owner): "some aspects of the UI are still hard to touch for some people".
// The panel is ~11.9 px/mm, so a 62 px track row is 5.2 mm; the common guidance for a finger
// target is 7-9 mm. 1.25x makes it 6.5 mm while a list still shows eight rows.

/// Big view's multiplier for row heights and touch zones, in percent.
pub const LARGE_PCT: i32 = 125;

#[cfg(not(test))]
static LARGE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
// Per test thread: a test that turns Big view on must not grow the rows under a parallel one.
#[cfg(test)]
thread_local! {
    static LARGE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub fn set_large(on: bool) {
    #[cfg(not(test))]
    LARGE.store(on, std::sync::atomic::Ordering::Relaxed);
    #[cfg(test)]
    LARGE.with(|l| l.set(on));
}

pub fn large() -> bool {
    #[cfg(not(test))]
    return LARGE.load(std::sync::atomic::Ordering::Relaxed);
    #[cfg(test)]
    return LARGE.with(|l| l.get());
}

/// `px` at the current control size: unchanged with Big view off.
pub fn ctl(px: i32) -> i32 {
    if large() {
        (px * LARGE_PCT + 50) / 100
    } else {
        px
    }
}

// ── row heights ────────────────────────────────────────────────────────────────────────────
// Upper-case functions on purpose: they replaced constants of the same name at every call site,
// and the name says what they still are — a row height, the same everywhere.

/// **A track row, wherever it appears** — the Songs tab, an album, an artist, a playlist, Up Next,
/// the folder tree. It was 68 in Songs, 62 on the album/artist/playlist/Up Next lists and 56 in
/// Folders; 62 because four of the five lists already used it (2026-09-12). The list area is
/// 645 px tall (`chrome::HEADER_BOTTOM` to `library::LIST_BOTTOM`), which is 10.4 rows.
#[allow(non_snake_case)]
pub fn TRACK_ROW_H() -> i32 {
    ctl(62)
}

/// A row standing for a GROUP of tracks — an album, an artist, a playlist — which carries stacked
/// art and a count, and is therefore legitimately taller than the track row inside it.
#[allow(non_snake_case)]
pub fn GROUP_ROW_H() -> i32 {
    ctl(68)
}

/// A settings-style row: a label, and a value or a switch on the right.
#[allow(non_snake_case)]
pub fn SETTING_ROW_H() -> i32 {
    ctl(64)
}

/// A picker row — a one-of-N list the user is choosing from (genre, codec, playlist actions).
/// Denser than a setting row on purpose: a picker is scanned, not lived in.
#[allow(non_snake_case)]
pub fn PICKER_ROW_H() -> i32 {
    ctl(56)
}

// ── what is still open ─────────────────────────────────────────────────────────────────────
//
// MIGRATION STATE, 2026-09-12. The row-label tier and the row heights are migrated: every
// primary row label on a list screen now names `ROW`, and every track row `TRACK_ROW_H`.
//
// NOT yet migrated, and each for a reason rather than by omission:
//   * the caption tier — 10, 11 and 12 px Regular, together the single biggest group of call
//     sites in the UI. Collapsing them to `CAPTION` (13) grows well over a hundred labels by up
//     to 30%, and those are exactly the strings that sit in fixed-width badges, status strips and
//     panel corners. That needs the 31-screen panel-overflow matrix re-baselined WITH a look at
//     the rendered PNGs, not a green test run.
//   * row subtitles at 13/14/15 — semantically `SECONDARY` (16), which is a growth rather than a
//     shrink for all three, so it belongs with the caption pass above.
//   * the seven treatments of the header's right-hand slot (audit C3), which is a `chrome::header`
//     API change, not a number.
