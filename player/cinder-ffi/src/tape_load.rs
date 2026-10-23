//! Cassette tapes off the drive for the Cassette style (`cinder_ui::tape`), laid out as Wampy lays
//! them out, so a tape or reel folder made for Wampy copies straight over:
//!
//!   cinder_cassette/tape/<name>/  the first picture by name (800×480) and an optional config.txt
//!   cinder_cassette/reel/<name>/  every picture, shown in name order
//!   cassette.txt in a music folder: `tape: <name>` and `reel: <name>` for the songs in it
//!
//! Pictures are JPEG as in Wampy; PNG and BMP are read too, through the cover decoder. Wampy's
//! compressed `.pkm` atlases are not read. Nothing of any tape or reel ships with Cinder.

use cinder_ui::tape::{parse_config, Tape};
use std::path::Path;

pub const ROOT: &str = "/contents/cinder_cassette";

/// More frames than this, or more pixels than `MAX_REEL_BYTES` across them, are left out: the
/// player shares its memory with everything else (Wampy's own warning).
const MAX_FRAMES: usize = 120;
const MAX_REEL_BYTES: usize = 24 << 20;

fn pictures(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut v: Vec<_> = std::fs::read_dir(dir)
        .map(|d| {
            d.flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.extension().and_then(|e| e.to_str()).is_some_and(|e| {
                        matches!(
                            e.to_ascii_lowercase().as_str(),
                            "jpg" | "jpeg" | "png" | "bmp"
                        )
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

/// The tape folders under `root`, by name.
pub fn names_in(root: &str) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(Path::new(root).join("tape"))
        .map(|d| {
            d.flatten()
                .filter(|e| e.path().is_dir())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

/// The `cassette.txt` beside `audio_path`, if any: the tape and the reel it names.
pub fn folder_choice(audio_path: &str) -> Option<(Option<String>, Option<String>)> {
    let txt = std::fs::read_to_string(Path::new(audio_path).parent()?.join("cassette.txt")).ok()?;
    let (mut tape, mut reel) = (None, None);
    for line in txt.lines() {
        if let Some((k, v)) = line.split_once(':') {
            let v = v.trim().to_string();
            match k.trim().to_ascii_lowercase().as_str() {
                "tape" if !v.is_empty() => tape = Some(v),
                "reel" if !v.is_empty() => reel = Some(v),
                _ => {}
            }
        }
    }
    Some((tape, reel))
}

/// Tape `name` from `root`, with its reel or `reel` in its place. None when the folder has no
/// readable picture.
pub fn load(root: &str, name: &str, reel: Option<&str>) -> Option<Tape> {
    let dir = Path::new(root).join("tape").join(name);
    let image = pictures(&dir).iter().find_map(|p| {
        std::fs::read(p)
            .ok()
            .and_then(|b| crate::art_load::decode(&b))
    })?;
    let image = if (image.w, image.h) == (800, 480) {
        image
    } else {
        image.scaled_to(800, 480)
    };
    let config = std::fs::read_to_string(dir.join("config.txt"))
        .map(|t| parse_config(&t))
        .unwrap_or_default();
    let reel_name = reel.unwrap_or(&config.reel).to_string();
    let mut frames = Vec::new();
    let mut bytes = 0;
    for p in pictures(&Path::new(root).join("reel").join(&reel_name)) {
        let Some(img) = std::fs::read(&p)
            .ok()
            .and_then(|b| crate::art_load::decode(&b))
        else {
            continue;
        };
        bytes += img.rgb.len();
        if frames.len() == MAX_FRAMES || bytes > MAX_REEL_BYTES {
            eprintln!(
                "cinder-ffi: reel {reel_name}: stopped at {} frames (memory cap)",
                frames.len()
            );
            break;
        }
        frames.push(img);
    }
    eprintln!(
        "cinder-ffi: tape {name} loaded, reel {reel_name}: {} frame(s)",
        frames.len()
    );
    Some(Tape {
        image,
        reel: frames,
        config,
        name: name.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 2×2 24-bit BMP of one colour.
    fn bmp(rgb: [u8; 3]) -> Vec<u8> {
        let mut b = b"BM".to_vec();
        let data = [rgb[2], rgb[1], rgb[0], rgb[2], rgb[1], rgb[0], 0, 0].repeat(2);
        b.extend_from_slice(&(54 + data.len() as u32).to_le_bytes());
        b.extend_from_slice(&[0; 4]);
        b.extend_from_slice(&54u32.to_le_bytes());
        b.extend_from_slice(&40u32.to_le_bytes());
        b.extend_from_slice(&2i32.to_le_bytes());
        b.extend_from_slice(&2i32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&24u16.to_le_bytes());
        b.extend_from_slice(&[0; 24]);
        b.extend_from_slice(&data);
        b
    }

    #[test]
    fn a_wampy_tape_folder_loads_with_its_reel_and_folder_choice() {
        let root = std::env::temp_dir().join(format!("cinder-tape-{}", std::process::id()));
        let tape = root.join("tape/chf");
        let reel = root.join("reel/other");
        let music = root.join("music");
        for d in [&tape, &reel, &music] {
            std::fs::create_dir_all(d).unwrap();
        }
        std::fs::write(tape.join("cas.bmp"), bmp([255, 0, 0])).unwrap();
        std::fs::write(tape.join("config.txt"), "titlex: 5\n").unwrap();
        for i in 0..3 {
            std::fs::write(reel.join(format!("r{i}.bmp")), bmp([0, 0, 255])).unwrap();
        }
        std::fs::write(music.join("cassette.txt"), "tape: chf\nreel: other\n").unwrap();
        std::fs::create_dir_all(root.join("tape/empty")).unwrap();
        let r = root.to_str().unwrap();
        let got = load(r, "chf", None);
        let names = names_in(r);
        let choice = folder_choice(music.join("song.flac").to_str().unwrap());
        let empty = load(r, "empty", None);
        std::fs::remove_dir_all(&root).ok();
        let t = got.expect("the tape loads");
        assert_eq!((t.image.w, t.image.h), (800, 480), "scaled to the panel");
        assert_eq!(t.reel.len(), 3);
        assert_eq!(t.config.title.x, 5.0);
        assert_eq!(names, ["chf", "empty"]);
        assert_eq!(choice, Some((Some("chf".into()), Some("other".into()))));
        assert!(empty.is_none(), "a folder with no picture is not a tape");
    }
}
