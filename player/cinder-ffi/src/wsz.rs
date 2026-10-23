//! Winamp classic skins: a `.wsz` is a zip of BMP sheets (`cinder_ui::skin`).
//!
//! The first `.wsz` at the top of the internal drive, by name, is the skin. It is read only when
//! the Winamp style is chosen, once per start. A zip member is stored or deflated (the two methods
//! skin tools write); BMPs are 24/32-bit, or 1/4/8-bit with a palette, uncompressed or RLE.

use cinder_ui::art::Image;
use cinder_ui::skin::{self, Skin, MAIN_H, MAIN_W};

const MAX_MEMBER: usize = 4 * 1024 * 1024;

fn u16le(b: &[u8], o: usize) -> Option<usize> {
    Some(u16::from_le_bytes(b.get(o..o + 2)?.try_into().ok()?) as usize)
}
fn u32le(b: &[u8], o: usize) -> Option<usize> {
    Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?) as usize)
}

/// Every member of a zip as (name, bytes). Folders inside the zip are kept in the name.
pub fn unzip(z: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    // The end-of-central-directory record, within the last 64 KiB (its comment can be that long).
    let lo = z.len().saturating_sub(65_557);
    let Some(eocd) = (lo..z.len().saturating_sub(21))
        .rev()
        .find(|&i| z[i..i + 4] == [0x50, 0x4b, 0x05, 0x06])
    else {
        return out;
    };
    let (Some(n), Some(mut p)) = (u16le(z, eocd + 10), u32le(z, eocd + 16)) else {
        return out;
    };
    for _ in 0..n {
        if z.get(p..p + 4) != Some(&[0x50, 0x4b, 0x01, 0x02][..]) {
            break;
        }
        let (
            Some(method),
            Some(csize),
            Some(usize_),
            Some(nlen),
            Some(xlen),
            Some(clen),
            Some(lho),
        ) = (
            u16le(z, p + 10),
            u32le(z, p + 20),
            u32le(z, p + 24),
            u16le(z, p + 28),
            u16le(z, p + 30),
            u16le(z, p + 32),
            u32le(z, p + 42),
        )
        else {
            break;
        };
        let name =
            String::from_utf8_lossy(z.get(p + 46..p + 46 + nlen).unwrap_or(&[])).into_owned();
        p += 46 + nlen + xlen + clen;
        if usize_ > MAX_MEMBER || name.ends_with('/') {
            continue;
        }
        let (Some(ln), Some(lx)) = (u16le(z, lho + 26), u16le(z, lho + 28)) else {
            continue;
        };
        let start = lho + 30 + ln + lx;
        let Some(data) = z.get(start..start + csize) else {
            continue;
        };
        let bytes = match method {
            0 => data.to_vec(),
            8 => match miniz_oxide::inflate::decompress_to_vec_with_limit(data, MAX_MEMBER) {
                Ok(v) => v,
                Err(_) => continue,
            },
            _ => continue,
        };
        out.push((name, bytes));
    }
    out
}

/// A BMP sheet: uncompressed 24/32-bit, or 1/4/8-bit with a palette; bottom-up or top-down.
pub fn decode_bmp(b: &[u8]) -> Option<Image> {
    if b.get(0..2)? != b"BM" {
        return None;
    }
    let data_off = u32le(b, 0x0A)?;
    let hdr = u32le(b, 0x0E)?;
    if hdr < 40 {
        return None;
    }
    let w = i32::from_le_bytes(b.get(0x12..0x16)?.try_into().ok()?);
    let hr = i32::from_le_bytes(b.get(0x16..0x1A)?.try_into().ok()?);
    let bpp = u16le(b, 0x1C)?;
    let compression = u32le(b, 0x1E)?;
    // 0 plain; 1 RLE8 and 2 RLE4, which some paint programs saved skins with.
    let rle = match (compression, bpp) {
        (0, _) => false,
        (1, 8) | (2, 4) => true,
        _ => return None,
    };
    if w <= 0 || w > 4096 || hr == 0 || hr.unsigned_abs() > 4096 || (rle && hr < 0) {
        return None;
    }
    let (w, h) = (w as usize, hr.unsigned_abs() as usize);
    let palette: Vec<[u8; 3]> = if bpp <= 8 {
        let used = u32le(b, 0x2E)?;
        let n = if used == 0 { 1 << bpp } else { used.min(256) };
        let at = 14 + hdr;
        (0..n)
            .map(|i| {
                let q = b.get(at + i * 4..at + i * 4 + 3)?;
                Some([q[2], q[1], q[0]])
            })
            .collect::<Option<_>>()?
    } else {
        Vec::new()
    };
    if !matches!(bpp, 1 | 4 | 8 | 24 | 32) {
        return None;
    }
    if rle {
        let idx = unrle(b.get(data_off..)?, w, h, bpp == 4);
        let mut rgb = Vec::with_capacity(w * h * 3);
        for y in 0..h {
            for &i in &idx[(h - 1 - y) * w..(h - y) * w] {
                rgb.extend_from_slice(palette.get(i as usize)?);
            }
        }
        return Some(Image { w, h, rgb });
    }
    let stride = (w * bpp).div_ceil(32) * 4;
    if data_off.checked_add(stride.checked_mul(h)?)? > b.len() {
        return None;
    }
    let mut rgb = vec![0u8; w * h * 3];
    for y in 0..h {
        let row = data_off + if hr < 0 { y } else { h - 1 - y } * stride;
        for x in 0..w {
            let px = match bpp {
                24 | 32 => {
                    let s = row + x * bpp / 8;
                    [b[s + 2], b[s + 1], b[s]]
                }
                _ => {
                    let bit = x * bpp;
                    let byte = b[row + bit / 8];
                    let idx = (byte >> (8 - bpp - bit % 8)) & ((1u16 << bpp) - 1) as u8;
                    *palette.get(idx as usize)?
                }
            };
            rgb[(y * w + x) * 3..(y * w + x) * 3 + 3].copy_from_slice(&px);
        }
    }
    Some(Image { w, h, rgb })
}

/// An RLE8 (or, `four`, RLE4) pixel stream as palette indices, bottom row first. A run past the
/// edge is clipped and a truncated stream leaves index 0, so a damaged sheet still draws.
fn unrle(d: &[u8], w: usize, h: usize, four: bool) -> Vec<u8> {
    let mut out = vec![0u8; w * h];
    let (mut x, mut y, mut i) = (0usize, 0usize, 0usize);
    let mut put = |x: &mut usize, y: usize, v: u8| {
        if *x < w && y < h {
            out[y * w + *x] = v;
        }
        *x += 1;
    };
    while i + 1 < d.len() && y < h {
        let (n, v) = (d[i] as usize, d[i + 1]);
        i += 2;
        if n > 0 {
            for k in 0..n {
                let px = if four {
                    if k % 2 == 0 {
                        v >> 4
                    } else {
                        v & 15
                    }
                } else {
                    v
                };
                put(&mut x, y, px);
            }
            continue;
        }
        match v {
            0 => (x, y) = (0, y + 1),
            1 => break,
            2 => {
                let (dx, dy) = (*d.get(i).unwrap_or(&0), *d.get(i + 1).unwrap_or(&0));
                i += 2;
                (x, y) = (x + dx as usize, y + dy as usize);
            }
            n => {
                let n = n as usize;
                let bytes = if four { n.div_ceil(2) } else { n };
                for k in 0..n {
                    let byte = *d.get(i + if four { k / 2 } else { k }).unwrap_or(&0);
                    let px = if !four {
                        byte
                    } else if k % 2 == 0 {
                        byte >> 4
                    } else {
                        byte & 15
                    };
                    put(&mut x, y, px);
                }
                i += bytes + bytes % 2; // literals pad to a 16-bit boundary
            }
        }
    }
    out
}

/// The skin in `z` (a whole `.wsz`), named `name`. None without a readable main.bmp.
pub fn skin_from(z: &[u8], name: &str) -> Option<Skin> {
    let members = unzip(z);
    let sheet = |want: &str| {
        members
            .iter()
            .find(|(n, _)| {
                n.rsplit(['/', '\\'])
                    .next()
                    .is_some_and(|b| b.eq_ignore_ascii_case(want))
            })
            .and_then(|(_, d)| decode_bmp(d))
    };
    let main = sheet("main.bmp")?.scaled_to(MAIN_W, MAIN_H);
    let txt = |want: &str| {
        members
            .iter()
            .find(|(n, _)| {
                n.rsplit(['/', '\\'])
                    .next()
                    .is_some_and(|b| b.eq_ignore_ascii_case(want))
            })
            .map(|(_, d)| String::from_utf8_lossy(d).into_owned())
    };
    Some(Skin {
        main: Some(main),
        titlebar: sheet("titlebar.bmp"),
        cbuttons: sheet("cbuttons.bmp"),
        numbers: sheet("numbers.bmp").or_else(|| sheet("nums_ex.bmp")),
        text: sheet("text.bmp"),
        posbar: sheet("posbar.bmp"),
        shufrep: sheet("shufrep.bmp"),
        volume: sheet("volume.bmp"),
        balance: sheet("balance.bmp").or_else(|| sheet("volume.bmp")),
        playpaus: sheet("playpaus.bmp"),
        monoster: sheet("monoster.bmp"),
        pledit: sheet("pledit.bmp"),
        pl_colors: txt("pledit.txt").map_or(skin::PL_DEFAULT, |t| skin::parse_pledit(&t)),
        viscolor: txt("viscolor.txt").map_or(skin::VIS_DEFAULT, |t| skin::parse_viscolor(&t)),
        name: name.to_string(),
    })
}

/// The `.wsz` files in `dir`, by name.
pub fn names_in(dir: &str) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map(|d| {
            d.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.to_ascii_lowercase().ends_with(".wsz"))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// The skin after `cur` in `names`, wrapping; the first when `cur` is not there.
pub fn next_name<'a>(names: &'a [String], cur: &str) -> Option<&'a String> {
    let i = names.iter().position(|n| n == cur).map_or(0, |i| i + 1);
    names.get(i % names.len().max(1))
}

/// `want` from `dir` when it is there, else the first `.wsz` by name; None when there is none or
/// it is not a skin.
pub fn find_in(dir: &str, want: &str) -> Option<Skin> {
    let names = names_in(dir);
    let name = names.iter().find(|n| *n == want).or(names.first())?;
    let z = std::fs::read(std::path::Path::new(dir).join(name)).ok()?;
    let skin = skin_from(&z, name);
    eprintln!(
        "cinder-ffi: Winamp skin {name}: {}",
        if skin.is_some() {
            "loaded"
        } else {
            "no readable main.bmp"
        }
    );
    skin
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An 8-bit palette BMP, `w`×`h`, every pixel palette index (x + y) % 2: red / blue.
    fn bmp8(w: usize, h: usize) -> Vec<u8> {
        let stride = (w + 3) & !3;
        let data_off = 14 + 40 + 2 * 4;
        let mut b = Vec::new();
        b.extend_from_slice(b"BM");
        b.extend_from_slice(&((data_off + stride * h) as u32).to_le_bytes());
        b.extend_from_slice(&[0; 4]);
        b.extend_from_slice(&(data_off as u32).to_le_bytes());
        b.extend_from_slice(&40u32.to_le_bytes());
        b.extend_from_slice(&(w as i32).to_le_bytes());
        b.extend_from_slice(&(h as i32).to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&8u16.to_le_bytes());
        b.extend_from_slice(&[0; 4]); // BI_RGB
        b.extend_from_slice(&[0; 12]);
        b.extend_from_slice(&2u32.to_le_bytes()); // colours used
        b.extend_from_slice(&[0; 4]);
        b.extend_from_slice(&[0, 0, 255, 0, 255, 0, 0, 0]); // BGRA: red, blue
        for y in 0..h {
            let row: Vec<u8> = (0..stride).map(|x| ((x + (h - 1 - y)) % 2) as u8).collect();
            b.extend_from_slice(&row);
        }
        b
    }

    /// A zip of stored and deflated members, as a skin tool writes it.
    fn zip(members: &[(&str, &[u8], bool)]) -> Vec<u8> {
        let mut z = Vec::new();
        let mut cd = Vec::new();
        for (name, data, deflate) in members {
            let body = if *deflate {
                miniz_oxide::deflate::compress_to_vec(data, 6)
            } else {
                data.to_vec()
            };
            let off = z.len() as u32;
            let method: u16 = if *deflate { 8 } else { 0 };
            z.extend_from_slice(&[0x50, 0x4b, 0x03, 0x04, 20, 0, 0, 0]);
            z.extend_from_slice(&method.to_le_bytes());
            z.extend_from_slice(&[0; 8]); // time, date, crc (not checked)
            z.extend_from_slice(&(body.len() as u32).to_le_bytes());
            z.extend_from_slice(&(data.len() as u32).to_le_bytes());
            z.extend_from_slice(&(name.len() as u16).to_le_bytes());
            z.extend_from_slice(&0u16.to_le_bytes());
            z.extend_from_slice(name.as_bytes());
            z.extend_from_slice(&body);
            cd.extend_from_slice(&[0x50, 0x4b, 0x01, 0x02, 20, 0, 20, 0, 0, 0]);
            cd.extend_from_slice(&method.to_le_bytes());
            cd.extend_from_slice(&[0; 8]);
            cd.extend_from_slice(&(body.len() as u32).to_le_bytes());
            cd.extend_from_slice(&(data.len() as u32).to_le_bytes());
            cd.extend_from_slice(&(name.len() as u16).to_le_bytes());
            cd.extend_from_slice(&[0; 12]);
            cd.extend_from_slice(&off.to_le_bytes());
            cd.extend_from_slice(name.as_bytes());
        }
        let cd_off = z.len() as u32;
        z.extend_from_slice(&cd);
        z.extend_from_slice(&[0x50, 0x4b, 0x05, 0x06, 0, 0, 0, 0]);
        z.extend_from_slice(&(members.len() as u16).to_le_bytes());
        z.extend_from_slice(&(members.len() as u16).to_le_bytes());
        z.extend_from_slice(&(cd.len() as u32).to_le_bytes());
        z.extend_from_slice(&cd_off.to_le_bytes());
        z.extend_from_slice(&0u16.to_le_bytes());
        z
    }

    #[test]
    fn palette_bmps_decode_top_row_first() {
        let img = decode_bmp(&bmp8(3, 2)).unwrap();
        assert_eq!((img.w, img.h), (3, 2));
        assert_eq!(&img.rgb[0..3], &[255, 0, 0], "top-left is index 0: red");
        assert_eq!(&img.rgb[3..6], &[0, 0, 255], "then blue");
        assert_eq!(&img.rgb[9..12], &[0, 0, 255], "the next row starts blue");
    }

    #[test]
    fn a_skin_comes_out_of_a_wsz_in_a_folder_with_either_method() {
        let main = bmp8(275, 116);
        let nums = bmp8(99, 13);
        let z = zip(&[
            ("Base/MAIN.BMP", &main, true),
            ("Base/numbers.bmp", &nums, false),
        ]);
        let skin = skin_from(&z, "base.wsz").expect("main.bmp is enough");
        let m = skin.main.as_ref().unwrap();
        assert_eq!((m.w, m.h), (MAIN_W, MAIN_H), "scaled once, at load");
        assert_eq!(skin.numbers.as_ref().map(|n| n.w), Some(99));
        assert!(
            skin.cbuttons.is_none(),
            "a missing sheet is None, not a failure"
        );
        assert!(skin_from(&zip(&[("x.bmp", &nums, false)]), "x.wsz").is_none());
        assert!(skin_from(b"not a zip", "y.wsz").is_none());
    }

    #[test]
    fn the_skin_chip_steps_through_the_drive_and_wraps() {
        let names: Vec<String> = ["a.wsz", "b.wsz", "c.wsz"].map(String::from).to_vec();
        assert_eq!(
            next_name(&names, "a.wsz").map(|s| s.as_str()),
            Some("b.wsz")
        );
        assert_eq!(
            next_name(&names, "c.wsz").map(|s| s.as_str()),
            Some("a.wsz")
        );
        assert_eq!(
            next_name(&names, "gone.wsz").map(|s| s.as_str()),
            Some("a.wsz"),
            "a deleted choice starts again at the first"
        );
        assert_eq!(next_name(&[], "a.wsz"), None);

        let dir = std::env::temp_dir().join(format!("cinder-wsz-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let main = bmp8(275, 116);
        for n in ["b.wsz", "a.WSZ"] {
            std::fs::write(dir.join(n), zip(&[("main.bmp", &main, true)])).unwrap();
        }
        std::fs::write(dir.join("notes.txt"), b"").unwrap();
        let d = dir.to_str().unwrap();
        let got = (
            names_in(d),
            find_in(d, "b.wsz").map(|s| s.name),
            find_in(d, "").map(|s| s.name),
        );
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(got.0, ["a.WSZ", "b.wsz"]);
        assert_eq!(got.1.as_deref(), Some("b.wsz"), "the saved choice");
        assert_eq!(
            got.2.as_deref(),
            Some("a.WSZ"),
            "no choice: the first by name"
        );
    }

    #[test]
    fn rle_bmps_decode_like_plain_ones() {
        // A 4x2 RLE8 sheet: bottom row red ×4 (a run), top row red, blue, blue, red (literals
        // then a run). Palette and header as `bmp8`, compression 1.
        let mut b = bmp8(4, 2);
        let data_off = u32le(&b, 0x0A).unwrap();
        b[0x1E] = 1;
        b.truncate(data_off);
        b.extend_from_slice(&[4, 0, 0, 0, 0, 3, 0, 1, 1, 0, 1, 0, 0, 0, 0, 1]);
        let img = decode_bmp(&b).expect("RLE8 is read");
        let px = |x: usize, y: usize| &img.rgb[(y * 4 + x) * 3..(y * 4 + x) * 3 + 3];
        assert_eq!(px(0, 1), [255, 0, 0], "the bottom row's run");
        assert_eq!(px(1, 0), [0, 0, 255], "a literal on the top row");
        assert_eq!(px(3, 0), [255, 0, 0], "the run after the padded literals");
        // RLE4: a run of alternating nibbles.
        assert_eq!(unrle(&[4, 0x10, 0, 1], 4, 1, true), [1, 0, 1, 0]);
        // Truncated or past the edge: no panic, the rest is index 0.
        assert_eq!(unrle(&[9, 1, 0, 5, 1], 3, 2, false), [1, 1, 1, 0, 0, 0]);
    }
}
