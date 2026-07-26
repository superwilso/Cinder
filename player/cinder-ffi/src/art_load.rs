//! Album-cover loading + decode — resolves a track's art through the `images` table and
//! decodes it to a `cinder_ui::art::Image` (packed RGB).
//!
//! Three storage shapes (analysis/H_mediastore/RE_findings.md):
//!   1. `bmpfile` — a pre-rendered bitmap file the stock scanner wrote (BMP; fastest path).
//!   2. `value` as BLOB — the image bytes stored inline in the DB row.
//!   3. `value` as TEXT + `dataoffset`/`datasize` — the art blob EMBEDDED in that source music
//!      file (ID3 APIC / FLAC PICTURE / MP4 covr) — raw JPEG or PNG bytes at that offset.
//! Everything is best-effort: any parse/IO failure returns None and the UI keeps its
//! gradient-placeholder fallback. Decode runs once per track change, never per frame.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

use cinder_ui::art::Image;

/// Cap for a single art blob (sanity against a corrupt datasize — covers are ≤ a few MB).
const MAX_BLOB: u64 = 16 * 1024 * 1024;
/// Cap on DECODED dimensions. Was 2048 on the "mastering accident" theory — and then the user's
/// real library turned out to hold 22 albums with covers up to 3600×3600 (Bandcamp-style masters),
/// every one silently cover-less. 4096² is a 50 MB transient decode; the device has 467 MB with
/// ~250 MB free+reclaimable (measured 2026-07-26), and the decode is serialized (one at a time in
/// the art-cache builder, or one per track change), so the peak is safe. The check runs on the
/// HEADER dimensions before any allocation — see decode_jpeg — so covers beyond even this cap cost
/// a few KB of header parse, not a decode.
const MAX_DIM: usize = 4096;

/// Resolve + decode art for `object_id`. Returns the image at its NATIVE decoded size
/// (caller pre-scales to draw sizes). Logs ONE diagnostic line per call to stderr (→
/// cinderhome.log on device; called once per track change, so this is cheap) — it pinpoints
/// which shape the real DB row has and where the pipeline stops if a cover doesn't render.
pub fn load(db: &cinder_db::Db, object_id: i64) -> Option<Image> {
    let art = match db.art_for_object(object_id) {
        Ok(Some(a)) => a,
        Ok(None) => {
            eprintln!("[cinder-ffi] art: obj={object_id} no images row");
            return None;
        }
        Err(e) => {
            eprintln!("[cinder-ffi] art: obj={object_id} query error: {e}");
            return None;
        }
    };
    let bmp = art.bmpfile.as_deref().unwrap_or("");
    let bmp_exists = !bmp.is_empty() && std::fs::metadata(bmp).is_ok();
    let magic = if !art.source_path.is_empty() && art.data_size > 0 {
        read_file_range(&art.source_path, art.data_offset.max(0) as u64, 4)
            .map(|b| format!("{:02X}{:02X}{:02X}{:02X}", b[0], b[1], b[2], b[3]))
            .unwrap_or_else(|| "unreadable".into())
    } else {
        "-".into()
    };
    eprintln!(
        "[cinder-ffi] art: obj={object_id} bmpfile={bmp:?} exists={bmp_exists} blob_len={} \
         src={:?} off={} size={} magic={magic}",
        art.blob.as_ref().map_or(0, |b| b.len()),
        art.source_path, art.data_offset, art.data_size
    );
    // Path 1: pre-rendered bitmap file.
    if bmp_exists {
        if let Some(img) = read_file_range(bmp, 0, 0).and_then(|b| decode(&b)) {
            return Some(img);
        }
    }
    // Path 2: image bytes stored INLINE in the DB (images.value is a BLOB).
    if let Some(blob) = art.blob.as_deref() {
        if blob.len() as u64 <= MAX_BLOB {
            if let Some(img) = decode(blob) {
                return Some(img);
            }
        }
    }
    // Path 3: embedded blob in the source file at (offset, size).
    if !art.source_path.is_empty() && art.data_size > 0 {
        let bytes = read_file_range(&art.source_path, art.data_offset.max(0) as u64, art.data_size as u64)?;
        return decode(&bytes);
    }
    None
}

/// Read `len` bytes at `off` (len 0 = whole file), size-capped.
fn read_file_range(path: &str, off: u64, len: u64) -> Option<Vec<u8>> {
    let mut f = File::open(path).ok()?;
    let total = f.metadata().ok()?.len();
    if off >= total {
        return None;
    }
    let want = if len == 0 { total - off } else { len.min(total - off) };
    if want == 0 || want > MAX_BLOB {
        return None;
    }
    f.seek(SeekFrom::Start(off)).ok()?;
    let mut buf = vec![0u8; want as usize];
    f.read_exact(&mut buf).ok()?;
    Some(buf)
}

/// Sniff the magic and decode JPEG / PNG / BMP into packed RGB.
pub fn decode(bytes: &[u8]) -> Option<Image> {
    match bytes {
        [0xFF, 0xD8, ..] => decode_jpeg(bytes),
        [0x89, b'P', b'N', b'G', ..] => decode_png(bytes),
        [b'B', b'M', ..] => decode_bmp(bytes),
        _ => None,
    }
}

fn decode_jpeg(bytes: &[u8]) -> Option<Image> {
    use zune_jpeg::zune_core::colorspace::ColorSpace;
    use zune_jpeg::zune_core::options::DecoderOptions;
    let opts = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGB);
    let mut dec = zune_jpeg::JpegDecoder::new_with_options(bytes, opts);
    // Dimension-gate on the HEADER before the pixel decode: the old order (decode, then check)
    // allocated the full RGB — 39 MB for a real 3600×3600 cover — only to throw it away.
    if let Err(e) = dec.decode_headers() {
        eprintln!("[cinder-ffi] art: jpeg header parse failed: {e}");
        return None;
    }
    let (w, h) = dec.dimensions()?;
    if w == 0 || h == 0 || w > MAX_DIM || h > MAX_DIM {
        eprintln!("[cinder-ffi] art: jpeg {w}x{h} exceeds cap {MAX_DIM} — skipped");
        return None;
    }
    let out = match dec.decode() {
        Ok(v) => v,
        Err(e) => {
            // Named because a silent None here cost a day: 22 albums looked "cover-less" when the
            // real story (oversize/grayscale) was only visible once this path said why it gave up.
            eprintln!("[cinder-ffi] art: jpeg {w}x{h} decode failed: {e}");
            return None;
        }
    };
    // The out-colorspace request is not a guarantee: a GRAYSCALE source (components=1 — real
    // covers in the user's library are mastered this way) comes back 1 byte/px regardless.
    // Match on the actual layout by exact length; anything unrecognized is logged, not guessed.
    let px = w * h;
    let rgb = if out.len() == px * 3 {
        out
    } else if out.len() == px {
        out.iter().flat_map(|&g| [g, g, g]).collect()
    } else if out.len() == px * 4 {
        out.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect()
    } else {
        eprintln!(
            "[cinder-ffi] art: jpeg {w}x{h} returned {} bytes (unrecognized layout) — skipped",
            out.len()
        );
        return None;
    };
    Some(Image { w, h, rgb })
}

fn decode_png(bytes: &[u8]) -> Option<Image> {
    let dec = png::Decoder::new(bytes);
    let mut reader = dec.read_info().ok()?;
    {
        // Gate on the HEADER dimensions before allocating the output buffer (a hostile
        // header would otherwise size the Vec at whatever it claims).
        let hi = reader.info();
        if hi.width == 0 || hi.height == 0 || hi.width as usize > MAX_DIM || hi.height as usize > MAX_DIM {
            return None;
        }
    }
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    let (w, h) = (info.width as usize, info.height as usize);
    if w == 0 || h == 0 || w > MAX_DIM || h > MAX_DIM {
        return None;
    }
    let rgb = match info.color_type {
        png::ColorType::Rgb => {
            buf.truncate(w * h * 3);
            buf
        }
        png::ColorType::Rgba => {
            let mut out = Vec::with_capacity(w * h * 3);
            for px in buf.chunks_exact(4).take(w * h) {
                out.extend_from_slice(&px[..3]);
            }
            out
        }
        png::ColorType::Grayscale => {
            let mut out = Vec::with_capacity(w * h * 3);
            for &g in buf.iter().take(w * h) {
                out.extend_from_slice(&[g, g, g]);
            }
            out
        }
        png::ColorType::GrayscaleAlpha => {
            let mut out = Vec::with_capacity(w * h * 3);
            for px in buf.chunks_exact(2).take(w * h) {
                out.extend_from_slice(&[px[0], px[0], px[0]]);
            }
            out
        }
        // Indexed is expanded to RGB by the png crate only with a transform; skip (rare for art).
        _ => return None,
    };
    if rgb.len() < w * h * 3 {
        return None;
    }
    Some(Image { w, h, rgb })
}

/// Minimal BMP reader: BITMAPINFOHEADER (or larger), uncompressed 24/32bpp, bottom-up or
/// top-down. That's what thumbnail caches write; anything else returns None.
fn decode_bmp(b: &[u8]) -> Option<Image> {
    if b.len() < 54 || &b[0..2] != b"BM" {
        return None;
    }
    let u32le = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
    let i32le = |o: usize| i32::from_le_bytes(b[o..o + 4].try_into().unwrap());
    let data_off = u32le(0x0A) as usize;
    let hdr_size = u32le(0x0E) as usize;
    if hdr_size < 40 {
        return None; // BITMAPCOREHEADER etc. — not produced by the caches we read
    }
    let w = i32le(0x12);
    let h_raw = i32le(0x16);
    let planes = u16::from_le_bytes(b[0x1A..0x1C].try_into().unwrap());
    let bpp = u16::from_le_bytes(b[0x1C..0x1E].try_into().unwrap());
    let compression = u32le(0x1E);
    if w <= 0 || w > 8192 || h_raw == 0 || h_raw.unsigned_abs() > 8192 || planes != 1 {
        return None;
    }
    if compression != 0 || (bpp != 24 && bpp != 32) {
        return None;
    }
    let (w, h) = (w as usize, h_raw.unsigned_abs() as usize);
    let top_down = h_raw < 0;
    let bytes_px = bpp as usize / 8;
    let stride = (w * bytes_px + 3) & !3; // rows padded to 4 bytes
    if data_off.checked_add(stride.checked_mul(h)?)? > b.len() {
        return None;
    }
    let mut rgb = vec![0u8; w * h * 3];
    for y in 0..h {
        let src_y = if top_down { y } else { h - 1 - y };
        let row = data_off + src_y * stride;
        for x in 0..w {
            let s = row + x * bytes_px;
            let d = (y * w + x) * 3;
            // BMP stores BGR(A)
            rgb[d] = b[s + 2];
            rgb[d + 1] = b[s + 1];
            rgb[d + 2] = b[s];
        }
    }
    Some(Image { w, h, rgb })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a tiny 24bpp bottom-up BMP: 2×2, rows padded to 4 bytes.
    fn tiny_bmp() -> Vec<u8> {
        let mut v = vec![0u8; 54 + 16];
        v[0] = b'B';
        v[1] = b'M';
        v[0x0A] = 54; // data offset
        v[0x0E] = 40; // BITMAPINFOHEADER
        v[0x12] = 2; // w
        v[0x16] = 2; // h (bottom-up)
        v[0x1A] = 1; // planes
        v[0x1C] = 24; // bpp
        // bottom row first: (B,G,R) blue px then red px
        let rows = [
            [(255u8, 0u8, 0u8), (0, 0, 255)],  // file row 0 = image BOTTOM: blue, red(BGR)
            [(0, 255, 0), (255, 255, 255)],    // file row 1 = image TOP: green, white
        ];
        for (ri, row) in rows.iter().enumerate() {
            let off = 54 + ri * 8; // stride = 2*3=6 → padded 8
            for (pi, &(b_, g, r)) in row.iter().enumerate() {
                v[off + pi * 3] = b_;
                v[off + pi * 3 + 1] = g;
                v[off + pi * 3 + 2] = r;
            }
        }
        v
    }

    #[test]
    fn bmp_decodes_bottom_up_bgr() {
        let img = decode(&tiny_bmp()).unwrap();
        assert_eq!((img.w, img.h), (2, 2));
        // top-left = green (file row 1 px 0, BGR 0,255,0 → RGB 0,255,0)
        assert_eq!(&img.rgb[0..3], &[0, 255, 0]);
        // bottom-left = blue (file row 0 px 0, BGR 255,0,0 → RGB 0,0,255)
        assert_eq!(&img.rgb[(2 * 1 + 0) * 3..(2 * 1 + 0) * 3 + 3], &[0, 0, 255]);
        // bottom-right = red
        assert_eq!(&img.rgb[(2 * 1 + 1) * 3..(2 * 1 + 1) * 3 + 3], &[255, 0, 0]);
    }

    #[test]
    fn garbage_is_rejected() {
        assert!(decode(&[0u8; 64]).is_none());
        assert!(decode(b"BMxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx").is_none());
    }

    #[test]
    fn scaled_to_shrinks() {
        let img = decode(&tiny_bmp()).unwrap();
        let s = img.scaled_to(1, 1);
        assert_eq!((s.w, s.h), (1, 1));
        assert_eq!(s.rgb.len(), 3);
    }
}
