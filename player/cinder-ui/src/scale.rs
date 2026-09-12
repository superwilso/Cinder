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

/// **A track row, wherever it appears** — the Songs tab, an album, an artist, a playlist, Up Next,
/// the folder tree. It was 68 in Songs, 62 on the album/artist/playlist/Up Next lists and 56 in
/// Folders, and `folders.rs` carried a comment claiming its 56 was "the same 56 the Songs tab
/// uses, so a track row looks the same wherever it is". The Songs tab was 68.
///
/// 62 because four of the five lists already used it, so the fewest screens move; the list area is
/// 645 px tall (`chrome::HEADER_BOTTOM` to `library::LIST_BOTTOM`), which is 10.4 rows.
pub const TRACK_ROW_H: i32 = 62;

/// A row standing for a GROUP of tracks — an album, an artist, a playlist — which carries stacked
/// art and a count, and is therefore legitimately taller than the track row inside it. Albums were
/// 68 and Artists/Playlists 70; one number for one object.
pub const GROUP_ROW_H: i32 = 68;

/// A settings-style row: a label, and a value or a switch on the right.
pub const SETTING_ROW_H: i32 = 64;

/// A picker row — a one-of-N list the user is choosing from (genre, codec, playlist actions).
/// Denser than a setting row on purpose: a picker is scanned, not lived in.
pub const PICKER_ROW_H: i32 = 56;

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
