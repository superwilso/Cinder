//! flac.rs — Cinder's own FLAC decoder (docs/PLAN_hardware_first.md, Phase 3).
//!
//! WHY OWN. PlayerService's libFLAC 1.3.2 cannot parse sample-size code 7 (32-bit, written by
//! FLAC ≥ 1.4) and hangs the service; the small Rust decoder (claxon) rejects the same code. This
//! one decodes 4- to 32-bit audio, predicting in i64 so a 32-bit file's 33-bit side channel and its
//! LPC sums cannot overflow.
//!
//! Integer only. Reads the STREAMINFO and SEEKTABLE blocks, skips a leading ID3v2 tag, checks every
//! frame header's CRC-8 (which is what makes the sync search trustworthy after a seek or damage).
//! ponytail: the frame footer's CRC-16 is read, not checked; a damaged frame plays as noise rather
//! than being dropped. Check it if a damaged file is ever reported.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::num::Wrapping;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Info {
    pub rate: u32,
    pub channels: u32,
    pub bits: u32,
    /// Samples per channel; 0 = unknown.
    pub total: u64,
    pub md5: [u8; 16],
    pub max_block: u32,
}

fn bad(m: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, m.to_string())
}

/// MSB-first bit reader over a buffered source, tracking the byte position for seeking.
struct Bits<R> {
    src: R,
    buf: Vec<u8>,
    pos: usize,
    len: usize,
    /// File offset of buf[0].
    base: u64,
    acc: u64,
    n: u32,
}

impl<R: Read + Seek> Bits<R> {
    fn new(src: R) -> Self {
        Bits {
            src,
            buf: vec![0; 64 * 1024],
            pos: 0,
            len: 0,
            base: 0,
            acc: 0,
            n: 0,
        }
    }
    fn byte(&mut self) -> io::Result<Option<u8>> {
        if self.pos == self.len {
            self.base += self.len as u64;
            self.len = self.src.read(&mut self.buf)?;
            self.pos = 0;
            if self.len == 0 {
                return Ok(None);
            }
        }
        self.pos += 1;
        Ok(Some(self.buf[self.pos - 1]))
    }
    fn refill(&mut self) -> io::Result<()> {
        while self.n <= 56 {
            match self.byte()? {
                Some(b) => {
                    self.acc |= (b as u64) << (56 - self.n);
                    self.n += 8;
                }
                None => break,
            }
        }
        Ok(())
    }
    /// Up to 56 bits.
    fn read(&mut self, k: u32) -> io::Result<u64> {
        if k == 0 {
            return Ok(0);
        }
        if self.n < k {
            self.refill()?;
            if self.n < k {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
        }
        let v = self.acc >> (64 - k);
        self.acc = if k == 64 { 0 } else { self.acc << k };
        self.n -= k;
        Ok(v)
    }
    /// Up to 64 bits, sign-extended.
    fn signed(&mut self, k: u32) -> io::Result<i64> {
        if k == 0 {
            return Ok(0);
        }
        let v = if k > 56 {
            (self.read(k - 32)? << 32) | self.read(32)?
        } else {
            self.read(k)?
        };
        Ok(((v << (64 - k)) as i64) >> (64 - k))
    }
    /// Zeros before the next 1.
    fn unary(&mut self) -> io::Result<u64> {
        let mut count = 0u64;
        loop {
            if self.n == 0 {
                self.refill()?;
                if self.n == 0 {
                    return Err(io::ErrorKind::UnexpectedEof.into());
                }
            }
            let lz = self.acc.leading_zeros();
            if lz < self.n {
                self.acc <<= lz + 1;
                self.n -= lz + 1;
                return Ok(count + lz as u64);
            }
            count += self.n as u64;
            self.acc = 0;
            self.n = 0;
        }
    }
    fn align(&mut self) {
        let drop = self.n % 8;
        self.acc <<= drop;
        self.n -= drop;
    }
    /// File offset of the next unread byte (byte-aligned callers only).
    fn tell(&self) -> u64 {
        self.base + self.pos as u64 - (self.n / 8) as u64
    }
    fn seek(&mut self, off: u64) -> io::Result<()> {
        self.src.seek(SeekFrom::Start(off))?;
        self.base = off;
        self.pos = 0;
        self.len = 0;
        self.acc = 0;
        self.n = 0;
        Ok(())
    }
}

fn crc8(data: &[u8]) -> u8 {
    let mut c = 0u8;
    for &b in data {
        c ^= b;
        for _ in 0..8 {
            c = if c & 0x80 != 0 {
                (c << 1) ^ 0x07
            } else {
                c << 1
            };
        }
    }
    c
}

struct Header {
    block: usize,
    chan: u32,
    bits: u32,
    /// First sample of the frame, per channel.
    first: u64,
}

pub struct Decoder<R> {
    r: Bits<R>,
    pub info: Info,
    audio_start: u64,
    seektable: Vec<(u64, u64)>,
    ch: Vec<Vec<i64>>,
    res: Vec<i64>,
    /// Interleaved samples decoded but not yet handed out, and how many of them are gone.
    pending: Vec<i32>,
    taken: usize,
}

pub fn open(path: &str) -> io::Result<Decoder<File>> {
    Decoder::new(File::open(path)?)
}

impl<R: Read + Seek> Decoder<R> {
    pub fn new(src: R) -> io::Result<Self> {
        let mut r = Bits::new(src);
        let mut magic = r.read(32)? as u32;
        if magic >> 8 == 0x494433 {
            // ID3v2: 3 more header bytes, then a syncsafe size; a footer adds 10.
            let _ver = r.read(16)?;
            let flags = r.read(8)?;
            let mut size = 0u64;
            for _ in 0..4 {
                size = (size << 7) | (r.read(8)? & 0x7f);
            }
            if flags & 0x10 != 0 {
                size += 10;
            }
            let at = r.tell();
            r.seek(at + size)?;
            magic = r.read(32)? as u32;
        }
        if magic != 0x664C_6143 {
            return Err(bad("not a FLAC file"));
        }
        let mut info = Info::default();
        let mut seektable = Vec::new();
        loop {
            let last = r.read(1)? == 1;
            let kind = r.read(7)?;
            let len = r.read(24)?;
            let at = r.tell();
            match kind {
                0 => {
                    let _min_block = r.read(16)?;
                    info.max_block = r.read(16)? as u32;
                    let _frames = r.read(48)?;
                    info.rate = r.read(20)? as u32;
                    info.channels = r.read(3)? as u32 + 1;
                    info.bits = r.read(5)? as u32 + 1;
                    info.total = r.read(36)?;
                    for b in info.md5.iter_mut() {
                        *b = r.read(8)? as u8;
                    }
                }
                3 => {
                    for _ in 0..len / 18 {
                        let sample = (r.read(32)? << 32) | r.read(32)?;
                        let off = (r.read(32)? << 32) | r.read(32)?;
                        let _n = r.read(16)?;
                        if sample != u64::MAX {
                            seektable.push((sample, off));
                        }
                    }
                }
                _ => {}
            }
            r.seek(at + len)?;
            if last {
                break;
            }
        }
        if info.rate == 0 || info.bits < 4 || info.bits > 32 {
            return Err(bad("no usable STREAMINFO"));
        }
        let audio_start = r.tell();
        let n = info.channels as usize;
        Ok(Decoder {
            r,
            info,
            audio_start,
            seektable,
            ch: vec![Vec::new(); n],
            res: Vec::new(),
            pending: Vec::new(),
            taken: 0,
        })
    }

    /// The next frame header at or after the reader's byte position, CRC-8 checked.
    fn header(&mut self, search: bool) -> io::Result<Option<Header>> {
        self.r.align();
        loop {
            let start = self.r.tell();
            let mut raw = Vec::with_capacity(16);
            let b0 = match self.r.read(8) {
                Ok(b) => b as u8,
                Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
                Err(e) => return Err(e),
            };
            if b0 != 0xFF {
                if search {
                    continue;
                }
                return Err(bad("lost frame sync"));
            }
            let parsed = self.parse_header(b0, &mut raw);
            match parsed {
                Ok(h) => return Ok(Some(h)),
                Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
                Err(e) => {
                    if !search {
                        return Err(e);
                    }
                    self.r.seek(start + 1)?;
                }
            }
        }
    }

    fn parse_header(&mut self, b0: u8, raw: &mut Vec<u8>) -> io::Result<Header> {
        raw.push(b0);
        let byte = |r: &mut Bits<R>, raw: &mut Vec<u8>| -> io::Result<u64> {
            let b = r.read(8)?;
            raw.push(b as u8);
            Ok(b)
        };
        let b1 = byte(&mut self.r, raw)?;
        if b1 & 0xFE != 0xF8 {
            return Err(bad("not a frame header"));
        }
        let variable = b1 & 1 == 1;
        let b2 = byte(&mut self.r, raw)?;
        let b3 = byte(&mut self.r, raw)?;
        let (bs_code, sr_code) = ((b2 >> 4) as u32, (b2 & 15) as u32);
        let (ch_code, ss_code) = ((b3 >> 4) as u32, ((b3 >> 1) & 7) as u32);
        if b3 & 1 != 0 || ch_code > 10 || ss_code == 3 || bs_code == 0 || sr_code == 15 {
            return Err(bad("reserved value in a frame header"));
        }
        // The frame or sample number, UTF-8 style.
        let lead = byte(&mut self.r, raw)?;
        let extra = (lead as u8).leading_ones();
        if extra == 1 || extra > 7 {
            return Err(bad("bad coded number"));
        }
        let mut num = if extra == 0 {
            lead
        } else {
            lead & (0x7F >> extra)
        };
        for _ in 1..extra {
            let b = byte(&mut self.r, raw)?;
            if b & 0xC0 != 0x80 {
                return Err(bad("bad coded number"));
            }
            num = (num << 6) | (b & 0x3F);
        }
        let block = match bs_code {
            1 => 192,
            2..=5 => 576 << (bs_code - 2),
            6 => byte(&mut self.r, raw)? as usize + 1,
            7 => ((byte(&mut self.r, raw)? << 8) | byte(&mut self.r, raw)?) as usize + 1,
            _ => 256 << (bs_code - 8),
        };
        // The rate itself is the STREAMINFO's; only its extra bytes have to be read past.
        match sr_code {
            12 => {
                byte(&mut self.r, raw)?;
            }
            13 | 14 => {
                byte(&mut self.r, raw)?;
                byte(&mut self.r, raw)?;
            }
            _ => {}
        }
        let crc = self.r.read(8)? as u8;
        if crc != crc8(raw) {
            return Err(bad("frame header CRC"));
        }
        let bits = match ss_code {
            0 => self.info.bits,
            1 => 8,
            2 => 12,
            4 => 16,
            5 => 20,
            6 => 24,
            _ => 32,
        };
        let chan = if ch_code < 8 { ch_code + 1 } else { 2 };
        if chan != self.info.channels || block > 65535 {
            return Err(bad("frame does not match the stream"));
        }
        // Fixed block size counts frames; a variable one counts samples.
        let first = if variable {
            num
        } else {
            num * self.info.max_block.max(1) as u64
        };
        Ok(Header {
            block,
            chan: ch_code,
            bits,
            first,
        })
    }

    fn residual(&mut self, block: usize, order: usize) -> io::Result<()> {
        let method = self.r.read(2)?;
        if method > 1 {
            return Err(bad("reserved residual coding"));
        }
        let (pbits, escape) = if method == 0 { (4, 15) } else { (5, 31) };
        let porder = self.r.read(4)? as u32;
        let parts = 1usize << porder;
        if block >> porder < order || (porder > 0 && !block.is_multiple_of(parts)) {
            return Err(bad("bad residual partition"));
        }
        self.res.clear();
        for p in 0..parts {
            let n = (block >> porder) - if p == 0 { order } else { 0 };
            let param = self.r.read(pbits)? as u32;
            if param == escape {
                let raw = self.r.read(5)? as u32;
                for _ in 0..n {
                    let v = self.r.signed(raw)?;
                    self.res.push(v);
                }
            } else {
                for _ in 0..n {
                    let q = self.r.unary()?;
                    let v = (q << param) | self.r.read(param)?;
                    self.res.push(((v >> 1) as i64) ^ -((v & 1) as i64));
                }
            }
        }
        Ok(())
    }

    fn subframe(&mut self, c: usize, block: usize, bits: u32) -> io::Result<()> {
        if self.r.read(1)? != 0 {
            return Err(bad("subframe padding bit"));
        }
        let kind = self.r.read(6)? as usize;
        let wasted = if self.r.read(1)? == 1 {
            self.r.unary()? as u32 + 1
        } else {
            0
        };
        if wasted >= bits {
            return Err(bad("wasted bits"));
        }
        let bits = bits - wasted;
        let mut s = std::mem::take(&mut self.ch[c]);
        s.clear();
        match kind {
            0 => {
                let v = self.r.signed(bits)?;
                s.resize(block, v);
            }
            1 => {
                for _ in 0..block {
                    s.push(self.r.signed(bits)?);
                }
            }
            8..=12 => {
                let order = kind - 8;
                for _ in 0..order {
                    s.push(self.r.signed(bits)?);
                }
                self.residual(block, order)?;
                for (i, &e) in self.res.iter().enumerate() {
                    let n = i + order;
                    // Wrapping: a damaged frame may overflow; real ones never get near i64.
                    let w = |k: usize| Wrapping(s[n - k]);
                    let p = match order {
                        0 => Wrapping(0),
                        1 => w(1),
                        2 => Wrapping(2) * w(1) - w(2),
                        3 => Wrapping(3) * w(1) - Wrapping(3) * w(2) + w(3),
                        _ => Wrapping(4) * w(1) - Wrapping(6) * w(2) + Wrapping(4) * w(3) - w(4),
                    };
                    s.push((p + Wrapping(e)).0);
                }
            }
            32..=63 => {
                let order = kind - 31;
                for _ in 0..order {
                    s.push(self.r.signed(bits)?);
                }
                let precision = self.r.read(4)? as u32 + 1;
                if precision == 16 {
                    return Err(bad("LPC precision"));
                }
                let shift = self.r.signed(5)?;
                if shift < 0 {
                    return Err(bad("negative LPC shift"));
                }
                let mut coef = [0i64; 32];
                for k in coef.iter_mut().take(order) {
                    *k = self.r.signed(precision)?;
                }
                self.residual(block, order)?;
                for (i, &e) in self.res.iter().enumerate() {
                    let n = i + order;
                    let mut sum = 0i64;
                    for (j, &k) in coef[..order].iter().enumerate() {
                        sum = sum.wrapping_add(k.wrapping_mul(s[n - 1 - j]));
                    }
                    s.push((sum >> shift).wrapping_add(e));
                }
            }
            _ => return Err(bad("reserved subframe type")),
        }
        if s.len() != block {
            return Err(bad("subframe length"));
        }
        if wasted > 0 {
            for v in s.iter_mut() {
                *v <<= wasted;
            }
        }
        self.ch[c] = s;
        Ok(())
    }

    /// Decodes the next frame into `pending`. Ok(false) at the end of the stream.
    fn frame(&mut self) -> io::Result<bool> {
        let h = match self.header(false)? {
            Some(h) => h,
            None => return Ok(false),
        };
        self.frame_body(&h)?;
        Ok(true)
    }

    fn frame_body(&mut self, h: &Header) -> io::Result<()> {
        let n = self.info.channels as usize;
        for c in 0..n {
            // The side channel carries one bit more.
            let side = match h.chan {
                8 | 10 => c == 1,
                9 => c == 0,
                _ => false,
            };
            self.subframe(c, h.block, h.bits + side as u32)?;
        }
        self.r.align();
        let _crc16 = self.r.read(16)?;
        if n == 2 && h.chan >= 8 {
            let (a, b) = self.ch.split_at_mut(1);
            let (x, y) = (&mut a[0], &mut b[0]);
            for i in 0..h.block {
                let (l, r) = match h.chan {
                    8 => (x[i], x[i] - y[i]),
                    9 => (x[i] + y[i], y[i]),
                    _ => {
                        let mid = (x[i] << 1) | (y[i] & 1);
                        ((mid + y[i]) >> 1, (mid - y[i]) >> 1)
                    }
                };
                x[i] = l;
                y[i] = r;
            }
        }
        self.pending.clear();
        self.taken = 0;
        self.pending.reserve(h.block * n);
        for i in 0..h.block {
            for c in 0..n {
                self.pending.push(self.ch[c][i] as i32);
            }
        }
        Ok(())
    }

    /// Fills `out` with interleaved samples (at the stream's own bit depth, right-aligned).
    /// Returns frames written; 0 at the end.
    pub fn read(&mut self, out: &mut [i32]) -> io::Result<usize> {
        let n = self.info.channels as usize;
        let mut w = 0;
        while w + n <= out.len() {
            if self.taken == self.pending.len() {
                if !self.frame()? {
                    break;
                }
                continue;
            }
            let k = (self.pending.len() - self.taken).min(out.len() - w) / n * n;
            out[w..w + k].copy_from_slice(&self.pending[self.taken..self.taken + k]);
            self.taken += k;
            w += k;
        }
        Ok(w / n)
    }

    /// Positions the stream so the next `read` starts at `sample` (per channel).
    pub fn seek(&mut self, sample: u64) -> io::Result<()> {
        // Start from the best SEEKTABLE point, or bisect the file for a frame that starts at or
        // before the target.
        let point = self
            .seektable
            .iter()
            .rev()
            .find(|&&(s, _)| s <= sample)
            .copied();
        let (mut lo, mut hi) = (self.audio_start, self.r.src.seek(SeekFrom::End(0))?);
        if let Some((_, off)) = point {
            lo = self.audio_start.saturating_add(off);
        } else {
            while hi.saturating_sub(lo) > 64 * 1024 {
                let mid = lo + (hi - lo) / 2;
                self.r.seek(mid)?;
                match self.header(true)? {
                    Some(h) if h.first <= sample => lo = mid,
                    _ => hi = mid,
                }
            }
        }
        self.r.seek(lo)?;
        loop {
            let h = match self.header(true)? {
                Some(h) => h,
                None => {
                    self.pending.clear();
                    self.taken = 0;
                    return Ok(());
                }
            };
            if h.first + h.block as u64 > sample {
                self.frame_body(&h)?;
                let skip = sample.saturating_sub(h.first) as usize;
                self.taken = (skip * self.info.channels as usize).min(self.pending.len());
                return Ok(());
            }
            // Skip this frame's body by decoding it (bodies have no length field).
            self.frame_body(&h)?;
        }
    }
}

// ── The C interface cinder-home's playback engine uses ────────────────────────────────────────

/// Opens `path`; null if it is not a FLAC this decoder can play.
#[no_mangle]
pub extern "C" fn cinder_flac_open(path: *const libc::c_char) -> *mut Decoder<File> {
    let p = unsafe { crate::cstr(path) };
    match open(&p) {
        Ok(d) => Box::into_raw(Box::new(d)),
        Err(e) => {
            eprintln!("cinder-ffi: flac: {p}: {e}");
            std::ptr::null_mut()
        }
    }
}

/// Rate, channels, bits per sample and total samples per channel (0 = unknown).
#[no_mangle]
pub extern "C" fn cinder_flac_info(
    h: *const Decoder<File>,
    rate: *mut u32,
    channels: *mut u32,
    bits: *mut u32,
    total: *mut u64,
) {
    let Some(d) = (unsafe { h.as_ref() }) else {
        return;
    };
    unsafe {
        *rate = d.info.rate;
        *channels = d.info.channels;
        *bits = d.info.bits;
        *total = d.info.total;
    }
}

/// Up to `frames` interleaved frames into `out` (samples right-aligned at the stream's depth).
/// Returns frames written, 0 at the end, -1 on a damaged stream.
#[no_mangle]
pub extern "C" fn cinder_flac_read(h: *mut Decoder<File>, out: *mut i32, frames: usize) -> isize {
    let Some(d) = (unsafe { h.as_mut() }) else {
        return -1;
    };
    let out = unsafe { std::slice::from_raw_parts_mut(out, frames * d.info.channels as usize) };
    match d.read(out) {
        Ok(n) => n as isize,
        Err(e) => {
            eprintln!("cinder-ffi: flac: {e}");
            -1
        }
    }
}

/// Next read starts at `sample` (per channel). 0 on success.
#[no_mangle]
pub extern "C" fn cinder_flac_seek(h: *mut Decoder<File>, sample: u64) -> libc::c_int {
    let Some(d) = (unsafe { h.as_mut() }) else {
        return -1;
    };
    if d.seek(sample).is_ok() {
        0
    } else {
        -1
    }
}

#[no_mangle]
pub extern "C" fn cinder_flac_close(h: *mut Decoder<File>) {
    if !h.is_null() {
        drop(unsafe { Box::from_raw(h) });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::process::{Command, Stdio};

    /// The FLAC MD5 is over the decoded samples as little-endian signed integers, (bits+7)/8 bytes
    /// each, interleaved.
    fn md5_of(path: &str) -> (Info, String, u64) {
        let mut d = open(path).unwrap();
        let info = d.info.clone();
        let bytes = info.bits.div_ceil(8) as usize;
        let mut child = Command::new("md5sum")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut stdin = child.stdin.take().unwrap();
        let mut buf = vec![0i32; 4096 * info.channels as usize];
        let mut frames = 0u64;
        loop {
            let n = d.read(&mut buf).unwrap();
            if n == 0 {
                break;
            }
            frames += n as u64;
            let mut out = Vec::with_capacity(n * info.channels as usize * bytes);
            for &s in &buf[..n * info.channels as usize] {
                out.extend_from_slice(&s.to_le_bytes()[..bytes]);
            }
            stdin.write_all(&out).unwrap();
        }
        drop(stdin);
        let o = child.wait_with_output().unwrap();
        (
            info,
            String::from_utf8_lossy(&o.stdout)[..32].to_string(),
            frames,
        )
    }

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    /// Every FLAC under CINDER_FLAC_DIR (or the fixtures) decodes to its own STREAMINFO MD5.
    #[test]
    fn decodes_to_streaminfo_md5() {
        let dir = std::env::var("CINDER_FLAC_DIR")
            .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/tests/flac").to_string());
        let mut paths: Vec<_> = std::fs::read_dir(&dir)
            .map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.path()).collect())
            .unwrap_or_default();
        paths.retain(|p| p.extension().is_some_and(|e| e == "flac"));
        paths.sort();
        for p in &paths {
            let path = p.to_str().unwrap();
            let (info, got, frames) = md5_of(path);
            assert_eq!(
                got,
                hex(&info.md5),
                "{path}: {}-bit, {} ch",
                info.bits,
                info.channels
            );
            assert_eq!(frames, info.total, "{path}: sample count");
        }
        eprintln!("{} FLAC files decoded to their own MD5", paths.len());
    }

    /// A seek lands on the exact sample: compare against a straight decode.
    #[test]
    fn seek_is_sample_exact() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/flac");
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for e in rd.filter_map(|e| e.ok()) {
            let path = e.path();
            if path.extension().is_none_or(|x| x != "flac") {
                continue;
            }
            let path = path.to_str().unwrap();
            let mut d = open(path).unwrap();
            let ch = d.info.channels as usize;
            let mut all = vec![0i32; d.info.total as usize * ch];
            let n = d.read(&mut all).unwrap();
            assert_eq!(n as u64, d.info.total);
            for target in [
                0u64,
                1,
                4095,
                4096,
                12345,
                d.info.total * 2 / 3,
                d.info.total - 1,
            ] {
                if target >= d.info.total {
                    continue;
                }
                d.seek(target).unwrap();
                let mut one = vec![0i32; 64 * ch];
                let got = d.read(&mut one).unwrap();
                let t = target as usize * ch;
                let want = &all[t..(t + got * ch).min(all.len())];
                assert_eq!(&one[..want.len()], want, "{path}: seek to {target}");
            }
        }
    }

    /// A damaged file must end in an error, never a panic or a hang: the device build aborts on
    /// panic, which takes cinder-home down. CINDER_FLAC_FUZZ=N runs N mutations per fixture.
    #[test]
    fn damaged_files_never_panic() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/flac");
        let n: u32 = std::env::var("CINDER_FLAC_FUZZ")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(40);
        let mut seed = 0x9e37_79b9_7f4a_7c15u64;
        let mut rnd = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for e in std::fs::read_dir(dir).unwrap().filter_map(|e| e.ok()) {
            let clean = std::fs::read(e.path()).unwrap();
            for i in 0..n {
                let mut b = clean.clone();
                match i % 4 {
                    // Flip a few bytes anywhere (headers included).
                    0 => (0..1 + rnd() % 8).for_each(|_| {
                        let at = rnd() as usize % b.len();
                        b[at] ^= 1 << (rnd() % 8);
                    }),
                    // Random bytes in the first 4 KB: STREAMINFO, SEEKTABLE, first frames.
                    1 => (0..1 + rnd() % 16).for_each(|_| {
                        let at = rnd() as usize % b.len().min(4096);
                        b[at] = rnd() as u8;
                    }),
                    // Truncated anywhere.
                    2 => b.truncate(rnd() as usize % b.len()),
                    // A run of garbage in the audio.
                    _ => {
                        let at = rnd() as usize % b.len();
                        let len = (rnd() as usize % 512).min(b.len() - at);
                        b[at..at + len].iter_mut().for_each(|x| *x = rnd() as u8);
                    }
                }
                let Ok(mut d) = Decoder::new(std::io::Cursor::new(b)) else {
                    continue;
                };
                let ch = (d.info.channels as usize).max(1);
                let mut buf = vec![0i32; 2048 * ch];
                let mut reads = 0;
                while let Ok(k) = d.read(&mut buf) {
                    if k == 0 {
                        break;
                    }
                    reads += 1;
                    assert!(reads < 100_000, "{:?} #{i}: read never ends", e.path());
                }
                let _ = d.seek(d.info.total / 2);
                let _ = d.read(&mut buf);
            }
        }
    }
}
