//! Settings ▸ Storage — how full each drive is, and what the music on them is made of.
//!
//! The Settings row used to be a reading ("41 / 56 GB, SD 108 / 119 GB") with nowhere to go. The
//! numbers behind a better answer were already on the player: Sony's scanner records every track's
//! file size, sample rate, bit depth and bitrate in MTPDB (`cinder_db::Db::file_facts`), so this
//! screen reads no files. [`summarise`] folds them once per library build; the screen only draws.
//!
//! LAYOUT IS ONE LIST, as on the Device screen: `items()` builds every row once, and the render and
//! the scroll extent both walk it. Nothing here is a control.

use crate::kit::{self, Row, Trail};
use crate::text::{self, Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::{fill_rect, stroke_rect, sty};
use crate::Canvas;

pub const ROW_H: i32 = kit::ROW_H;
pub const SECTION_H: i32 = kit::SECTION_H;
/// One drive in the hero: its name and figures, then the bar.
const DRIVE_H: i32 = 74;
const FOOTER_H: i32 = 60;

/// One track as the summary needs it. Built by the shell from `Track` + `FileFacts`.
#[derive(Clone, Debug, Default)]
pub struct TrackFile<'a> {
    pub path: &'a str,
    pub title: &'a str,
    pub album_id: i64,
    pub album: &'a str,
    pub ms: u64,
    /// Unix seconds the scanner first saw it; 0 = unknown.
    pub added: i64,
    pub bytes: u64,
    pub sample_rate: u32,
    pub bits: u32,
    pub bitrate: u32,
    pub hires: bool,
}

/// What the screen shows, folded once per library build.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Summary {
    pub tracks: u32,
    pub ms: u64,
    pub bytes: u64,
    /// Tracks and bytes on internal storage and on the SD card.
    pub internal: (u32, u64),
    pub sd: (u32, u64),
    /// ("FLAC", tracks, bytes), biggest first.
    pub formats: Vec<(String, u32, u64)>,
    /// (Hz, tracks), lowest first. 0 Hz (not recorded) is left out.
    pub rates: Vec<(u32, u32)>,
    /// (bits, tracks), lowest first. 0 (not recorded, or a lossy file) is left out.
    pub depths: Vec<(u32, u32)>,
    pub hires: u32,
    /// Mean of the recorded bitrates, in kbit/s; 0 when none is recorded.
    pub avg_kbps: u32,
    pub longest: Option<(String, u64)>,
    pub biggest_album: Option<(String, u64)>,
    pub added_30d: u32,
    /// The SD card's size and free space in MB, read when the library was built. 0 = no card.
    pub sd_total_mb: i32,
    pub sd_free_mb: i32,
}

impl Summary {
    /// A believable library for the host previews: about the owner's, rounded.
    pub fn sample() -> Self {
        Summary {
            tracks: 4646,
            ms: 1_150_000_000,
            bytes: 142_300_000_000,
            internal: (141, 4_700_000_000),
            sd: (4505, 137_600_000_000),
            formats: vec![
                ("FLAC".into(), 3920, 131_200_000_000),
                ("MP3".into(), 610, 9_800_000_000),
                ("AAC/ALAC".into(), 116, 1_300_000_000),
            ],
            rates: vec![(44_100, 4410), (48_000, 120), (96_000, 116)],
            depths: vec![(16, 3800), (24, 236)],
            hires: 236,
            avg_kbps: 912,
            longest: Some(("Echoes".into(), 1_412_000)),
            biggest_album: Some(("The Wall".into(), 1_900_000_000)),
            added_30d: 37,
            sd_total_mb: 243_000,
            sd_free_mb: 101_000,
        }
    }
}

/// "FLAC" from "/contents/MUSIC/a.flac". Files without an extension read "Other".
pub fn format_of(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    match name.rsplit_once('.') {
        Some((_, ext)) if !ext.is_empty() && ext.len() <= 5 => {
            match ext.to_ascii_lowercase().as_str() {
                "dsf" | "dff" => "DSD".into(),
                "m4a" | "mp4" => "AAC/ALAC".into(),
                "oma" | "aa3" => "ATRAC".into(),
                "aif" | "aiff" => "AIFF".into(),
                e => e.to_ascii_uppercase(),
            }
        }
        _ => "Other".into(),
    }
}

fn on_sd(path: &str) -> bool {
    path.starts_with("/contents_ext/") || path.starts_with("/data/mnt/external/")
}

/// Fold the library. `now_s` is the wall clock in Unix seconds, for "added in the last 30 days".
pub fn summarise(files: &[TrackFile], now_s: i64, sd_total_mb: i32, sd_free_mb: i32) -> Summary {
    use std::collections::BTreeMap;
    let mut s = Summary {
        sd_total_mb,
        sd_free_mb,
        ..Default::default()
    };
    let mut formats: BTreeMap<String, (u32, u64)> = BTreeMap::new();
    let mut rates: BTreeMap<u32, u32> = BTreeMap::new();
    let mut depths: BTreeMap<u32, u32> = BTreeMap::new();
    let mut albums: BTreeMap<i64, (&str, u64)> = BTreeMap::new();
    let (mut kbps_sum, mut kbps_n) = (0u64, 0u64);
    for f in files {
        s.tracks += 1;
        s.ms += f.ms;
        s.bytes += f.bytes;
        let side = if on_sd(f.path) {
            &mut s.sd
        } else {
            &mut s.internal
        };
        side.0 += 1;
        side.1 += f.bytes;
        let e = formats.entry(format_of(f.path)).or_default();
        e.0 += 1;
        e.1 += f.bytes;
        if f.sample_rate > 0 {
            *rates.entry(f.sample_rate).or_default() += 1;
        }
        if f.bits > 0 {
            *depths.entry(f.bits).or_default() += 1;
        }
        if f.hires {
            s.hires += 1;
        }
        if f.bitrate > 0 {
            kbps_sum += f.bitrate as u64 / 1000;
            kbps_n += 1;
        }
        if s.longest.as_ref().is_none_or(|(_, ms)| f.ms > *ms) && f.ms > 0 {
            s.longest = Some((f.title.to_string(), f.ms));
        }
        if f.album_id != 0 {
            let a = albums.entry(f.album_id).or_insert((f.album, 0));
            a.1 += f.bytes;
        }
        if f.added > 0 && now_s - f.added <= 30 * 86_400 && f.added <= now_s {
            s.added_30d += 1;
        }
    }
    s.formats = formats.into_iter().map(|(k, (n, b))| (k, n, b)).collect();
    s.formats.sort_by(|a, b| b.2.cmp(&a.2).then(b.1.cmp(&a.1)));
    s.rates = rates.into_iter().collect();
    s.depths = depths.into_iter().collect();
    s.avg_kbps = kbps_sum.checked_div(kbps_n).unwrap_or(0) as u32;
    s.biggest_album = albums
        .into_values()
        .filter(|(_, b)| *b > 0)
        .max_by_key(|(_, b)| *b)
        .map(|(n, b)| (n.to_string(), b));
    s
}

/// Everything the screen draws. Built by `nav` from the library and the pushed readings.
pub struct StorageView<'a> {
    pub sum: &'a Summary,
    pub albums: usize,
    pub artists: usize,
    pub genres: usize,
    /// Internal storage, in MB, as the shell's statvfs pushes it. `<= 0` = not read yet.
    pub int_total_mb: i32,
    pub int_free_mb: i32,
}

pub enum Item {
    Section(&'static str),
    /// Label, value, and a sub line (empty for none).
    Row(String, String, String),
}

fn row(label: &str, value: String) -> Item {
    Item::Row(label.to_string(), value, String::new())
}

/// THE layout. Both `render` and `content_height` walk this.
pub fn items(v: &StorageView) -> Vec<Item> {
    let s = v.sum;
    let tr = crate::i18n::text;
    let mut out = vec![
        Item::Section(tr("LIBRARY")),
        row(tr("Songs"), count(s.tracks as usize)),
        row(tr("Albums"), count(v.albums)),
        row(tr("Artists"), count(v.artists)),
        row(tr("Genres"), count(v.genres)),
        row(tr("Playing time"), duration_label(s.ms)),
        row(tr("Music size"), bytes_label(s.bytes)),
        Item::Section(tr("WHERE IT IS")),
        row(tr("Internal"), songs_and_size(s.internal.0, s.internal.1)),
        row(tr("SD card"), songs_and_size(s.sd.0, s.sd.1)),
    ];
    if !s.formats.is_empty() {
        out.push(Item::Section(tr("FORMATS")));
        for (name, n, b) in &s.formats {
            out.push(Item::Row(
                name.clone(),
                songs_and_size(*n, *b),
                String::new(),
            ));
        }
    }
    out.push(Item::Section(tr("QUALITY")));
    out.push(row(tr("Hi-Res"), count(s.hires as usize)));
    for (hz, n) in &s.rates {
        out.push(Item::Row(
            rate_label(*hz),
            count(*n as usize),
            String::new(),
        ));
    }
    for (bits, n) in &s.depths {
        out.push(Item::Row(
            format!("{bits}-bit"),
            count(*n as usize),
            String::new(),
        ));
    }
    if s.avg_kbps > 0 {
        out.push(row(tr("Average bitrate"), format!("{} kbit/s", s.avg_kbps)));
    }
    out.push(Item::Section(tr("RECORDS")));
    if let Some((t, ms)) = &s.longest {
        out.push(Item::Row(
            tr("Longest song").into(),
            duration_label(*ms),
            t.clone(),
        ));
    }
    if let Some((a, b)) = &s.biggest_album {
        out.push(Item::Row(
            tr("Biggest album").into(),
            bytes_label(*b),
            a.clone(),
        ));
    }
    out.push(row(
        tr("Added in the last 30 days"),
        count(s.added_30d as usize),
    ));
    out
}

fn item_h(it: &Item) -> i32 {
    match it {
        Item::Section(_) => SECTION_H,
        Item::Row(..) => ROW_H,
    }
}

fn drives(v: &StorageView) -> usize {
    1 + (v.sum.sd_total_mb > 0) as usize
}

fn top(v: &StorageView) -> i32 {
    crate::chrome::HEADER_BOTTOM + kit::STRIP_H + DRIVE_H * drives(v) as i32 + 12
}

pub fn content_height(v: &StorageView) -> i32 {
    top(v) + items(v).iter().map(item_h).sum::<i32>() + FOOTER_H
}

pub fn max_scroll_px(v: &StorageView) -> i32 {
    (content_height(v) - crate::canvas::H as i32).max(0)
}

// ── formatting, unit-tested ────────────────────────────────────────────────────────────────────

/// 4646 -> "4,646".
pub fn count(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// Bytes as Windows shows them, so a drive reads the same here as in Explorer: "131 GB", "4.1 GB",
/// "820 MB", counted in 1024s (the internal drive is "55.9 GB" in both places).
pub fn bytes_label(b: u64) -> String {
    let gb = b as f64 / (1u64 << 30) as f64;
    if gb >= 100.0 {
        format!("{gb:.0} GB")
    } else if gb >= 1.0 {
        format!("{gb:.1} GB")
    } else {
        format!("{:.0} MB", b as f64 / (1u64 << 20) as f64)
    }
}

/// MB from statvfs, the same way: "55.9 GB".
pub fn mb_label(mb: i32) -> String {
    bytes_label(mb.max(0) as u64 * 1_048_576)
}

fn songs_and_size(n: u32, b: u64) -> String {
    if n == 0 {
        return "—".into();
    }
    format!("{} \u{b7} {}", count(n as usize), bytes_label(b))
}

/// "12 days 4 h", "5 h 20 min", "3:42".
pub fn duration_label(ms: u64) -> String {
    let s = ms / 1000;
    let (d, h, m) = (s / 86_400, s / 3600 % 24, s / 60 % 60);
    if d > 0 {
        format!("{d} {} {h} h", if d == 1 { "day" } else { "days" })
    } else if h > 0 {
        format!("{h} h {m} min")
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

/// 44100 -> "44.1 kHz", 96000 -> "96 kHz", 2822400 -> "DSD64".
pub fn rate_label(hz: u32) -> String {
    match hz {
        2_822_400 => "DSD64".into(),
        5_644_800 => "DSD128".into(),
        11_289_600 => "DSD256".into(),
        _ if hz.is_multiple_of(1000) => format!("{} kHz", hz / 1000),
        _ => format!("{:.1} kHz", hz as f64 / 1000.0),
    }
}

// ── drawing ────────────────────────────────────────────────────────────────────────────────────

/// One drive: name and "used of total" on a line, then a bar split into music, everything else,
/// and free space. The music share is the only accent on the screen.
#[allow(clippy::too_many_arguments)]
fn drive(
    c: &mut Canvas,
    t: &Theme,
    f: &FontSet,
    y: i32,
    name: &str,
    total_mb: i32,
    free_mb: i32,
    music: u64,
) {
    let label = sty(Family::Sans, Weight::SemiBold, 17.0, t.ink, 0.0);
    let figs = sty(Family::Sans, Weight::Regular, 15.0, t.dim, 0.0);
    text::draw(c, f, kit::LEFT as f32, (y + 22) as f32, name, &label);
    let (bx, bw) = (kit::LEFT, kit::RIGHT - kit::LEFT);
    if total_mb <= 0 {
        let s = crate::i18n::text("Not read yet");
        let w = text::measure(f, s, &figs);
        text::draw(c, f, kit::RIGHT as f32 - w, (y + 22) as f32, s, &figs);
        stroke_rect(c, bx, y + 34, bw, 16, t.ctrl(), 1);
        return;
    }
    let used_mb = (total_mb - free_mb.max(0)).max(0);
    let right = format!("{} / {}", mb_label(used_mb), mb_label(total_mb));
    let w = text::measure(f, &right, &figs);
    text::draw(c, f, kit::RIGHT as f32 - w, (y + 22) as f32, &right, &figs);

    stroke_rect(c, bx, y + 34, bw, 16, t.ctrl(), 1);
    let inner = bw - 4;
    let total_b = total_mb as u64 * 1_048_576;
    let used_w = (inner as i64 * used_mb as i64 / total_mb.max(1) as i64) as i32;
    let music_w = ((inner as u64 * music.min(total_b)) / total_b.max(1)) as i32;
    let music_w = music_w.min(used_w);
    if used_w > music_w {
        fill_rect(c, bx + 2 + music_w, y + 36, used_w - music_w, 12, t.dim);
    }
    if music_w > 0 {
        fill_rect(c, bx + 2, y + 36, music_w, 12, t.acc);
    }
    let cap = sty(
        Family::Sans,
        Weight::Regular,
        crate::scale::CAPTION,
        t.faint,
        0.0,
    );
    let other = (used_mb as u64 * 1_048_576).saturating_sub(music);
    let line = format!(
        "{} {} \u{b7} {} {} \u{b7} {} {}",
        crate::i18n::text("Music"),
        bytes_label(music),
        crate::i18n::text("Other"),
        bytes_label(other),
        crate::i18n::text("Free"),
        mb_label(free_mb),
    );
    let line = crate::widgets::fit(f, &line, &cap, bw as f32);
    text::draw(c, f, kit::LEFT as f32, (y + 68) as f32, &line, &cap);
}

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, v: &StorageView, scroll: i32) {
    c.fill(t.bg);
    let head_bottom = crate::chrome::header(c, t, f, crate::i18n::text("Storage"), None);
    c.set_clip_y(head_bottom, crate::canvas::H as i32);

    let s = v.sum;
    let strip_y = head_bottom - scroll;
    let strip = format!(
        "{} {} \u{b7} {}",
        count(s.tracks as usize),
        crate::i18n::text("SONGS"),
        bytes_label(s.bytes)
    );
    kit::strip(c, t, f, strip_y, &strip);

    let mut y = strip_y + kit::STRIP_H;
    drive(
        c,
        t,
        f,
        y,
        crate::i18n::text("Internal"),
        v.int_total_mb,
        v.int_free_mb,
        s.internal.1,
    );
    y += DRIVE_H;
    if s.sd_total_mb > 0 {
        drive(
            c,
            t,
            f,
            y,
            crate::i18n::text("SD card"),
            s.sd_total_mb,
            s.sd_free_mb,
            s.sd.1,
        );
    }

    let mut y = top(v) - scroll;
    for it in items(v) {
        match &it {
            Item::Section(name) => {
                kit::section_label(c, t, f, y, name, None);
            }
            Item::Row(label, value, sub) => {
                kit::row(
                    c,
                    t,
                    f,
                    y,
                    ROW_H,
                    &Row::new(label).sub(sub).trail(Trail::Value(value)),
                );
            }
        }
        y += item_h(&it);
    }

    let foot = sty(
        Family::Mono,
        Weight::Regular,
        crate::scale::CAPTION,
        t.faint,
        0.1,
    );
    let w = (kit::RIGHT - kit::LEFT) as f32;
    let note = crate::i18n::text("FROM SONY'S MUSIC DATABASE. RESCAN TO REFRESH.");
    // Wrapped, not cut: at a large text size it runs past the edge (`--cut 120`).
    let x = kit::LEFT as f32;
    crate::widgets::draw_wrap2(c, f, x, (y + 30) as f32, note, &foot, x + w, 18.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tf<'a>(
        path: &'a str,
        album_id: i64,
        ms: u64,
        bytes: u64,
        rate: u32,
        bits: u32,
    ) -> TrackFile<'a> {
        TrackFile {
            path,
            title: path,
            album_id,
            album: "A",
            ms,
            bytes,
            sample_rate: rate,
            bits,
            bitrate: 900_000,
            ..Default::default()
        }
    }

    #[test]
    fn summary_splits_drives_formats_and_quality() {
        let files = [
            tf("/contents/MUSIC/a.flac", 1, 200_000, 30_000_000, 44_100, 16),
            tf(
                "/contents_ext/MUSIC/b.FLAC",
                1,
                300_000,
                40_000_000,
                96_000,
                24,
            ),
            tf(
                "/contents_ext/MUSIC/c.mp3",
                2,
                100_000,
                5_000_000,
                44_100,
                0,
            ),
        ];
        let s = summarise(&files, 0, 120_000, 20_000);
        assert_eq!(s.tracks, 3);
        assert_eq!(s.internal, (1, 30_000_000));
        assert_eq!(s.sd, (2, 45_000_000));
        assert_eq!(s.formats[0], ("FLAC".into(), 2, 70_000_000));
        assert_eq!(s.formats[1], ("MP3".into(), 1, 5_000_000));
        assert_eq!(s.rates, vec![(44_100, 2), (96_000, 1)]);
        assert_eq!(s.depths, vec![(16, 1), (24, 1)]);
        assert_eq!(s.longest.as_ref().map(|l| l.1), Some(300_000));
        assert_eq!(s.biggest_album.as_ref().map(|a| a.1), Some(70_000_000));
        assert_eq!(s.avg_kbps, 900);
    }

    #[test]
    fn added_in_the_last_30_days_counts_only_recent_known_dates() {
        let now = 2_000_000_000;
        let mut a = tf("/contents/a.flac", 1, 1, 1, 0, 0);
        a.added = now - 86_400;
        let mut b = a.clone();
        b.added = now - 31 * 86_400;
        let c = tf("/contents/c.flac", 1, 1, 1, 0, 0); // added unknown
        assert_eq!(summarise(&[a, b, c], now, 0, 0).added_30d, 1);
    }

    #[test]
    fn labels() {
        assert_eq!(count(4646), "4,646");
        assert_eq!(count(999), "999");
        assert_eq!(bytes_label(131 << 30), "131 GB");
        assert_eq!(bytes_label(4_402_341_478), "4.1 GB");
        assert_eq!(bytes_label(820 << 20), "820 MB");
        assert_eq!(mb_label(57_270), "55.9 GB");
        assert_eq!(rate_label(44_100), "44.1 kHz");
        assert_eq!(rate_label(96_000), "96 kHz");
        assert_eq!(rate_label(2_822_400), "DSD64");
        assert_eq!(duration_label(3_723_000), "1 h 2 min");
        assert_eq!(duration_label(222_000), "3:42");
        assert_eq!(duration_label(5 * 86_400_000 + 3_600_000), "5 days 1 h");
        assert_eq!(format_of("/x/y.dsf"), "DSD");
        assert_eq!(format_of("/x/noext"), "Other");
    }
}
