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
    // ZCursor, not `bytes`. zune-jpeg 0.5 takes `T: ZByteReaderTrait` where 0.4 took `&[u8]`, and
    // the only no_std implementor is `ZCursor<impl AsRef<[u8]>>` (the other is `BufRead + Seek`,
    // which would drag std IO buffering into a decode of a slice we already hold whole).
    let mut dec = zune_jpeg::JpegDecoder::new_with_options(
        zune_jpeg::zune_core::bytestream::ZCursor::new(bytes),
        opts,
    );
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
    // Cursor, not `bytes`: png 0.18 requires `R: Read + Seek` on the reader (0.17 was happy with
    // a bare slice, which is Read but not Seek).
    let mut dec = png::Decoder::new(std::io::Cursor::new(bytes));
    // Normalise the sample format up front instead of hand-handling every variant below:
    //   STRIP_16 — 16-bit-per-channel PNGs (real in high-quality rips) otherwise arrive as
    //     w*h*6 bytes, and the RGB arm's `truncate(w*h*3)` would keep the first half of the
    //     interleaved high/low bytes, rendering the cover as NOISE rather than rejecting it.
    //     The grayscale arm had the same problem via `take(w*h)`.
    //   EXPAND — expands palette (Indexed) and sub-8-bit grayscale to full bytes. Indexed used to
    //     be skipped outright, so those covers simply never appeared.
    dec.set_transformations(png::Transformations::STRIP_16 | png::Transformations::EXPAND);
    let mut reader = dec.read_info().ok()?;
    {
        // Gate on the HEADER dimensions before allocating the output buffer (a hostile
        // header would otherwise size the Vec at whatever it claims).
        let hi = reader.info();
        if hi.width == 0 || hi.height == 0 || hi.width as usize > MAX_DIM || hi.height as usize > MAX_DIM {
            return None;
        }
    }
    // png 0.18 returns Option here: the size is width*height*channels and it now reports overflow
    // rather than wrapping. `?` is the right answer — a header whose buffer size does not fit a
    // usize is exactly the hostile input the dimension gate above exists for, and this is a second
    // net under it. 0.17 returned a bare usize and would have allocated on the wrapped value.
    let mut buf = vec![0u8; reader.output_buffer_size()?];
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

    /// Encode a 2x2 PNG with the given colour type and bit depth, so the decoder's handling of the
    /// awkward sample formats can be checked rather than assumed.
    fn png_bytes(color: png::ColorType, depth: png::BitDepth, data: &[u8], pal: Option<&[u8]>) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, 2, 2);
            enc.set_color(color);
            enc.set_depth(depth);
            if let Some(p) = pal {
                enc.set_palette(p.to_vec());
            }
            let mut w = enc.write_header().unwrap();
            w.write_image_data(data).unwrap();
        }
        out
    }

    /// 16-bit-per-channel PNGs are real in high-quality rips. Before STRIP_16 the RGB arm's
    /// `truncate(w*h*3)` kept the first half of the interleaved high/low bytes, so the cover
    /// decoded to NOISE instead of being rejected — the worst of both outcomes.
    #[test]
    fn png_16bit_decodes_to_real_pixels_not_noise() {
        // 2x2 RGB16: pure red, green, blue, white — big-endian samples.
        let px: Vec<u8> = [
            [0xffffu16, 0, 0], [0, 0xffff, 0],
            [0, 0, 0xffff], [0xffff, 0xffff, 0xffff],
        ]
        .iter()
        .flatten()
        .flat_map(|c| c.to_be_bytes())
        .collect();
        let img = decode_png(&png_bytes(png::ColorType::Rgb, png::BitDepth::Sixteen, &px, None))
            .expect("16-bit PNG must decode");
        assert_eq!((img.w, img.h), (2, 2));
        assert_eq!(img.rgb.len(), 2 * 2 * 3);
        assert_eq!(&img.rgb[0..3], &[255, 0, 0], "first pixel should be red, not a high/low byte pair");
        assert_eq!(&img.rgb[3..6], &[0, 255, 0]);
        assert_eq!(&img.rgb[9..12], &[255, 255, 255]);
    }

    /// Palette PNGs used to be skipped outright, so those covers simply never appeared.
    #[test]
    fn png_indexed_is_expanded_rather_than_skipped() {
        let pal = [255u8, 0, 0, /**/ 0, 0, 255]; // red, blue
        let img = decode_png(&png_bytes(
            png::ColorType::Indexed, png::BitDepth::Eight, &[0, 1, 1, 0], Some(&pal),
        ))
        .expect("indexed PNG must decode");
        assert_eq!(&img.rgb[0..3], &[255, 0, 0]);
        assert_eq!(&img.rgb[3..6], &[0, 0, 255]);
    }

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

    // ── JPEG ────────────────────────────────────────────────────────────────────────────────
    // This path had NO test at all until 2026-09-01, and it is the one the album art on this
    // user's device actually goes through — every cover in the log is `magic=FFD8FFE0`, a JPEG
    // embedded in a FLAC. The gap surfaced the hard way: a dependabot bump took zune-jpeg from
    // 0.4 to 0.5, the reader trait changed, and nothing here could tell whether the port was
    // right. Hand-built fixtures, 8x8, as byte literals rather than files — a JPEG is not
    // constructible inline the way the PNGs above are, but it is small enough to embed.
    const TINY_JPEG_RGB: &[u8] = &[
        // 761 bytes, produced by Pillow; 8x8.
        0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10, 0x4a, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x00, 0x00, 0x01,
        0x00, 0x01, 0x00, 0x00, 0xff, 0xdb, 0x00, 0x43, 0x00, 0x02, 0x01, 0x01, 0x01, 0x01, 0x01, 0x02,
        0x01, 0x01, 0x01, 0x02, 0x02, 0x02, 0x02, 0x02, 0x04, 0x03, 0x02, 0x02, 0x02, 0x02, 0x05, 0x04,
        0x04, 0x03, 0x04, 0x06, 0x05, 0x06, 0x06, 0x06, 0x05, 0x06, 0x06, 0x06, 0x07, 0x09, 0x08, 0x06,
        0x07, 0x09, 0x07, 0x06, 0x06, 0x08, 0x0b, 0x08, 0x09, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x06, 0x08,
        0x0b, 0x0c, 0x0b, 0x0a, 0x0c, 0x09, 0x0a, 0x0a, 0x0a, 0xff, 0xdb, 0x00, 0x43, 0x01, 0x02, 0x02,
        0x02, 0x02, 0x02, 0x02, 0x05, 0x03, 0x03, 0x05, 0x0a, 0x07, 0x06, 0x07, 0x0a, 0x0a, 0x0a, 0x0a,
        0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a,
        0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a,
        0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0xff, 0xc0,
        0x00, 0x11, 0x08, 0x00, 0x08, 0x00, 0x08, 0x03, 0x01, 0x22, 0x00, 0x02, 0x11, 0x01, 0x03, 0x11,
        0x01, 0xff, 0xc4, 0x00, 0x1f, 0x00, 0x00, 0x01, 0x05, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09,
        0x0a, 0x0b, 0xff, 0xc4, 0x00, 0xb5, 0x10, 0x00, 0x02, 0x01, 0x03, 0x03, 0x02, 0x04, 0x03, 0x05,
        0x05, 0x04, 0x04, 0x00, 0x00, 0x01, 0x7d, 0x01, 0x02, 0x03, 0x00, 0x04, 0x11, 0x05, 0x12, 0x21,
        0x31, 0x41, 0x06, 0x13, 0x51, 0x61, 0x07, 0x22, 0x71, 0x14, 0x32, 0x81, 0x91, 0xa1, 0x08, 0x23,
        0x42, 0xb1, 0xc1, 0x15, 0x52, 0xd1, 0xf0, 0x24, 0x33, 0x62, 0x72, 0x82, 0x09, 0x0a, 0x16, 0x17,
        0x18, 0x19, 0x1a, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a,
        0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4a, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5a,
        0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6a, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7a,
        0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89, 0x8a, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99,
        0x9a, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7, 0xa8, 0xa9, 0xaa, 0xb2, 0xb3, 0xb4, 0xb5, 0xb6, 0xb7,
        0xb8, 0xb9, 0xba, 0xc2, 0xc3, 0xc4, 0xc5, 0xc6, 0xc7, 0xc8, 0xc9, 0xca, 0xd2, 0xd3, 0xd4, 0xd5,
        0xd6, 0xd7, 0xd8, 0xd9, 0xda, 0xe1, 0xe2, 0xe3, 0xe4, 0xe5, 0xe6, 0xe7, 0xe8, 0xe9, 0xea, 0xf1,
        0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf7, 0xf8, 0xf9, 0xfa, 0xff, 0xc4, 0x00, 0x1f, 0x01, 0x00, 0x03,
        0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
        0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0xff, 0xc4, 0x00, 0xb5, 0x11, 0x00,
        0x02, 0x01, 0x02, 0x04, 0x04, 0x03, 0x04, 0x07, 0x05, 0x04, 0x04, 0x00, 0x01, 0x02, 0x77, 0x00,
        0x01, 0x02, 0x03, 0x11, 0x04, 0x05, 0x21, 0x31, 0x06, 0x12, 0x41, 0x51, 0x07, 0x61, 0x71, 0x13,
        0x22, 0x32, 0x81, 0x08, 0x14, 0x42, 0x91, 0xa1, 0xb1, 0xc1, 0x09, 0x23, 0x33, 0x52, 0xf0, 0x15,
        0x62, 0x72, 0xd1, 0x0a, 0x16, 0x24, 0x34, 0xe1, 0x25, 0xf1, 0x17, 0x18, 0x19, 0x1a, 0x26, 0x27,
        0x28, 0x29, 0x2a, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49,
        0x4a, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5a, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69,
        0x6a, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7a, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88,
        0x89, 0x8a, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9a, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6,
        0xa7, 0xa8, 0xa9, 0xaa, 0xb2, 0xb3, 0xb4, 0xb5, 0xb6, 0xb7, 0xb8, 0xb9, 0xba, 0xc2, 0xc3, 0xc4,
        0xc5, 0xc6, 0xc7, 0xc8, 0xc9, 0xca, 0xd2, 0xd3, 0xd4, 0xd5, 0xd6, 0xd7, 0xd8, 0xd9, 0xda, 0xe2,
        0xe3, 0xe4, 0xe5, 0xe6, 0xe7, 0xe8, 0xe9, 0xea, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf7, 0xf8, 0xf9,
        0xfa, 0xff, 0xda, 0x00, 0x0c, 0x03, 0x01, 0x00, 0x02, 0x11, 0x03, 0x11, 0x00, 0x3f, 0x00, 0x6f,
        0xfc, 0x1b, 0xbf, 0xf0, 0x5b, 0xfe, 0x1b, 0xeb, 0xfe, 0x16, 0xff, 0x00, 0xfc, 0x54, 0xbf, 0xf0,
        0x89, 0xff, 0x00, 0xc2, 0x27, 0xff, 0x00, 0x08, 0xff, 0x00, 0xfc, 0xb9, 0xfd, 0xbf, 0xed, 0x5f,
        0x6a, 0xfe, 0xd2, 0xff, 0x00, 0x6e, 0x1d, 0x9b, 0x7e, 0xcf, 0xfe, 0xd6, 0x77, 0xf6, 0xc7, 0x25,
        0x14, 0x57, 0xad, 0xf4, 0x87, 0xfa, 0x3c, 0x78, 0x3d, 0xc3, 0x9e, 0x31, 0x66, 0x79, 0x76, 0x5d,
        0x96, 0x7b, 0x3a, 0x34, 0xfd, 0x8f, 0x2c, 0x7d, 0xb6, 0x22, 0x56, 0xe6, 0xc3, 0xd2, 0x93, 0xd6,
        0x55, 0x5c, 0x9d, 0xe4, 0xdb, 0xd5, 0xf9, 0x6c, 0x7e, 0x7f, 0xc6, 0xfc, 0x11, 0xc2, 0xfe, 0x34,
        0xf1, 0x46, 0x23, 0x8c, 0xb8, 0xcb, 0x0f, 0xf5, 0xbc, 0xcb, 0x17, 0xc9, 0xed, 0x6a, 0xf3, 0xce,
        0x97, 0x3f, 0xb2, 0x84, 0x68, 0xd3, 0xfd, 0xdd, 0x19, 0x53, 0xa5, 0x1e, 0x5a, 0x54, 0xe1, 0x1f,
        0x76, 0x0a, 0xf6, 0xbb, 0xbc, 0x9b, 0x6f, 0xff, 0xd9,
    ];

    const TINY_JPEG_GRAY: &[u8] = &[
        // 371 bytes, produced by Pillow; 8x8.
        0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10, 0x4a, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x00, 0x00, 0x01,
        0x00, 0x01, 0x00, 0x00, 0xff, 0xdb, 0x00, 0x43, 0x00, 0x02, 0x01, 0x01, 0x01, 0x01, 0x01, 0x02,
        0x01, 0x01, 0x01, 0x02, 0x02, 0x02, 0x02, 0x02, 0x04, 0x03, 0x02, 0x02, 0x02, 0x02, 0x05, 0x04,
        0x04, 0x03, 0x04, 0x06, 0x05, 0x06, 0x06, 0x06, 0x05, 0x06, 0x06, 0x06, 0x07, 0x09, 0x08, 0x06,
        0x07, 0x09, 0x07, 0x06, 0x06, 0x08, 0x0b, 0x08, 0x09, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x06, 0x08,
        0x0b, 0x0c, 0x0b, 0x0a, 0x0c, 0x09, 0x0a, 0x0a, 0x0a, 0xff, 0xc0, 0x00, 0x0b, 0x08, 0x00, 0x08,
        0x00, 0x08, 0x01, 0x01, 0x11, 0x00, 0xff, 0xc4, 0x00, 0x1f, 0x00, 0x00, 0x01, 0x05, 0x01, 0x01,
        0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x02, 0x03, 0x04,
        0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0xff, 0xc4, 0x00, 0xb5, 0x10, 0x00, 0x02, 0x01, 0x03,
        0x03, 0x02, 0x04, 0x03, 0x05, 0x05, 0x04, 0x04, 0x00, 0x00, 0x01, 0x7d, 0x01, 0x02, 0x03, 0x00,
        0x04, 0x11, 0x05, 0x12, 0x21, 0x31, 0x41, 0x06, 0x13, 0x51, 0x61, 0x07, 0x22, 0x71, 0x14, 0x32,
        0x81, 0x91, 0xa1, 0x08, 0x23, 0x42, 0xb1, 0xc1, 0x15, 0x52, 0xd1, 0xf0, 0x24, 0x33, 0x62, 0x72,
        0x82, 0x09, 0x0a, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x34, 0x35,
        0x36, 0x37, 0x38, 0x39, 0x3a, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4a, 0x53, 0x54, 0x55,
        0x56, 0x57, 0x58, 0x59, 0x5a, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6a, 0x73, 0x74, 0x75,
        0x76, 0x77, 0x78, 0x79, 0x7a, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89, 0x8a, 0x92, 0x93, 0x94,
        0x95, 0x96, 0x97, 0x98, 0x99, 0x9a, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7, 0xa8, 0xa9, 0xaa, 0xb2,
        0xb3, 0xb4, 0xb5, 0xb6, 0xb7, 0xb8, 0xb9, 0xba, 0xc2, 0xc3, 0xc4, 0xc5, 0xc6, 0xc7, 0xc8, 0xc9,
        0xca, 0xd2, 0xd3, 0xd4, 0xd5, 0xd6, 0xd7, 0xd8, 0xd9, 0xda, 0xe1, 0xe2, 0xe3, 0xe4, 0xe5, 0xe6,
        0xe7, 0xe8, 0xe9, 0xea, 0xf1, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf7, 0xf8, 0xf9, 0xfa, 0xff, 0xda,
        0x00, 0x08, 0x01, 0x01, 0x00, 0x00, 0x3f, 0x00, 0x6f, 0xfc, 0xa9, 0x73, 0xff, 0x00, 0x57, 0x27,
        0xff, 0x00, 0x0d, 0x27, 0xff, 0x00, 0x72, 0x77, 0xfc, 0x23, 0xbf, 0xf0, 0x8f, 0x7f, 0xe0, 0xcf,
        0xed, 0x9f, 0x68, 0xfe, 0xdc, 0xff, 0x00, 0xa6, 0x3e, 0x57, 0xd9, 0x7f, 0xe5, 0xa7, 0x99, 0xf2,
        0x7f, 0xff, 0xd9,
    ];

    #[test]
    fn jpeg_rgb_decodes_with_the_right_dimensions_and_channel_order() {
        let img = decode(TINY_JPEG_RGB).expect("a valid 8x8 JPEG must decode");
        assert_eq!((img.w, img.h), (8, 8));
        assert_eq!(img.rgb.len(), 8 * 8 * 3, "must come back as packed RGB");
        // Quadrant centres. JPEG is lossy at any quality, so assert on which channel DOMINATES
        // rather than on exact values — that is what catches a channel swap or a stride error,
        // and it does not care about the encoder's rounding.
        let at = |x: usize, y: usize| {
            let i = (y * 8 + x) * 3;
            (img.rgb[i], img.rgb[i + 1], img.rgb[i + 2])
        };
        let (r, g, b) = at(1, 1);
        assert!(r > g + 40 && r > b + 40, "top-left should be red, got {r},{g},{b}");
        let (r, g, b) = at(6, 1);
        assert!(g > r + 40 && g > b + 40, "top-right should be green, got {r},{g},{b}");
        let (r, g, b) = at(1, 6);
        assert!(b > r + 40 && b > g + 40, "bottom-left should be blue, got {r},{g},{b}");
    }

    #[test]
    fn jpeg_grayscale_is_expanded_to_rgb_not_rejected() {
        // The out-colorspace request is not a guarantee: a 1-component source comes back one
        // byte per pixel whatever we asked for, and decode_jpeg fans it out to r=g=b. Real covers
        // in the library are mastered this way, and this is the branch that keeps them from
        // being logged as an "unrecognized layout" and dropped.
        let img = decode(TINY_JPEG_GRAY).expect("a grayscale JPEG must decode, not be skipped");
        assert_eq!((img.w, img.h), (8, 8));
        assert_eq!(img.rgb.len(), 8 * 8 * 3, "grayscale must be fanned out to packed RGB");
        for (i, px) in img.rgb.chunks_exact(3).enumerate() {
            assert!(
                px[0] == px[1] && px[1] == px[2],
                "px {i} is {px:?} — a grayscale expansion must leave r==g==b"
            );
        }
        // The fixture is a checkerboard, so both extremes have to survive the round trip.
        let at = |x: usize, y: usize| img.rgb[(y * 8 + x) * 3];
        assert!(at(1, 1) < 64, "dark quadrant came back at {}", at(1, 1));
        assert!(at(6, 1) > 192, "light quadrant came back at {}", at(6, 1));
    }

    #[test]
    fn a_truncated_jpeg_is_refused_rather_than_half_decoded() {
        // Covers are read out of a FLAC at a recorded offset and length; a wrong length is the
        // realistic corruption, not random bytes. It must return None, not a partial image.
        assert!(decode(&TINY_JPEG_RGB[..TINY_JPEG_RGB.len() / 2]).is_none());
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
