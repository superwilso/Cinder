//! A PNG writer small enough to read in one sitting, for `gui::screenshots`.
//!
//! The installer has no dependencies on purpose (see gui.rs), and a screenshot is the only image it
//! ever writes, so this stores the pixels uncompressed: zlib "stored" blocks, which every PNG
//! decoder must accept. The files are large; tools/render_installer_screenshots.sh recompresses
//! them before they reach the repository, and nothing here ships inside an install.

const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1A, b'\n'];
/// The most bytes one stored deflate block can hold.
const STORED_MAX: usize = 0xFFFF;

/// Encode 8-bit RGB pixels, rows from the top, as a PNG file.
pub fn encode_rgb(width: u32, height: u32, rgb: &[u8]) -> Vec<u8> {
    assert!(width > 0 && height > 0, "an image needs at least one pixel");
    let row = width as usize * 3;
    assert_eq!(rgb.len(), row * height as usize, "pixel buffer does not match {width}x{height} RGB");

    // Filter type 0 (None) in front of every scanline.
    let mut raw = Vec::with_capacity((row + 1) * height as usize);
    for line in rgb.chunks_exact(row) {
        raw.push(0);
        raw.extend_from_slice(line);
    }

    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // 8-bit, truecolour, deflate, standard filters, no interlace

    let mut out = SIGNATURE.to_vec();
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &zlib_stored(&raw));
    chunk(&mut out, b"IEND", &[]);
    out
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

/// A zlib stream made of uncompressed ("stored") deflate blocks.
fn zlib_stored(data: &[u8]) -> Vec<u8> {
    let mut z = Vec::with_capacity(data.len() + data.len() / STORED_MAX * 5 + 11);
    z.extend_from_slice(&[0x78, 0x01]); // deflate, 32K window; 0x7801 is a multiple of 31, as required
    let mut blocks = data.chunks(STORED_MAX).peekable();
    while let Some(b) = blocks.next() {
        z.push(u8::from(blocks.peek().is_none())); // BFINAL on the last block, BTYPE 00 = stored
        let len = b.len() as u16;
        z.extend_from_slice(&len.to_le_bytes());
        z.extend_from_slice(&(!len).to_le_bytes());
        z.extend_from_slice(b);
    }
    z.extend_from_slice(&adler32(data).to_be_bytes());
    z
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &b in bytes {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

fn adler32(bytes: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    // 5552 is the most bytes that can be summed before `b` could overflow a u32.
    for block in bytes.chunks(5552) {
        for &x in block {
            a += u32::from(x);
            b += a;
        }
        a %= 65521;
        b %= 65521;
    }
    (b << 16) | a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksums_match_their_published_check_values() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    /// Undo the stored blocks: the inverse of `zlib_stored`, and all a decoder does with them.
    fn inflate_stored(z: &[u8]) -> Vec<u8> {
        assert_eq!(&z[..2], &[0x78, 0x01]);
        let (mut i, mut out) = (2, Vec::new());
        loop {
            let last = z[i] & 1 == 1;
            let len = u16::from_le_bytes([z[i + 1], z[i + 2]]);
            assert_eq!(u16::from_le_bytes([z[i + 3], z[i + 4]]), !len);
            let len = len as usize;
            out.extend_from_slice(&z[i + 5..i + 5 + len]);
            i += 5 + len;
            if last {
                break;
            }
        }
        assert_eq!(&z[i..], &adler32(&out).to_be_bytes());
        out
    }

    /// Every chunk, with its CRC checked.
    fn chunks(png: &[u8]) -> Vec<([u8; 4], Vec<u8>)> {
        assert_eq!(&png[..8], &SIGNATURE);
        let (mut i, mut v) = (8, Vec::new());
        while i < png.len() {
            let n = u32::from_be_bytes(png[i..i + 4].try_into().unwrap()) as usize;
            let kind: [u8; 4] = png[i + 4..i + 8].try_into().unwrap();
            let crc = u32::from_be_bytes(png[i + 8 + n..i + 12 + n].try_into().unwrap());
            assert_eq!(crc, crc32(&png[i + 4..i + 8 + n]), "bad CRC on {}", String::from_utf8_lossy(&kind));
            v.push((kind, png[i + 8..i + 8 + n].to_vec()));
            i += 12 + n;
        }
        v
    }

    #[test]
    fn a_small_image_round_trips() {
        let rgb = [255, 0, 0, 0, 255, 0, 0, 0, 255, 9, 9, 9];
        let png = encode_rgb(2, 2, &rgb);
        let c = chunks(&png);
        assert_eq!(c.iter().map(|(k, _)| *k).collect::<Vec<_>>(), [*b"IHDR", *b"IDAT", *b"IEND"]);
        assert_eq!(c[0].1, [0, 0, 0, 2, 0, 0, 0, 2, 8, 2, 0, 0, 0]);
        assert_eq!(inflate_stored(&c[1].1), [0, 255, 0, 0, 0, 255, 0, 0, 0, 0, 255, 9, 9, 9]);
        assert_eq!(&png[png.len() - 4..], &[0xAE, 0x42, 0x60, 0x82]); // IEND's CRC, the same in every PNG
    }

    #[test]
    fn a_window_sized_image_spans_many_blocks() {
        let (w, h) = (744usize, 581usize);
        let rgb: Vec<u8> = (0..w * h * 3).map(|i| (i % 251) as u8).collect();
        let c = chunks(&encode_rgb(w as u32, h as u32, &rgb));
        let raw = inflate_stored(&c[1].1);
        assert!(raw.len() > STORED_MAX * 10);
        assert_eq!(raw.len(), (w * 3 + 1) * h);
        for (y, line) in raw.chunks_exact(w * 3 + 1).enumerate() {
            assert_eq!(line[0], 0);
            assert_eq!(&line[1..], &rgb[y * w * 3..(y + 1) * w * 3]);
        }
    }
}
