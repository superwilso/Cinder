//! Sony's NW-WM1A volume-curve tables, fetched from Sony's own update server.
//!
//! The "Smooth (NW-WM1A)" and "Walkman One" curves need three of Sony's files that the NW-A50
//! firmware does not carry and that Cinder may not ship. They are in Sony's NW-WM1 3.02 firmware,
//! a 119 MB disk image on the same server as the A50's. All three sit in one stretch of 36 KB of
//! it, so one HTTP Range request is enough, and nothing else in the image is downloaded.
//!
//! ONLY WHEN ASKED. Choosing one of those two curves is the request (the option says so); nothing
//! here runs otherwise (see `release.rs` on network requests).
//!
//! HOW THE NUMBERS WERE FOUND (2026-10-22, from `NW-WM1_V3_02.dmg`, SHA-256 `f680c2c4…`). The
//! image holds `NW_WM_FW.UPG` (SHA-256 `314a5005…`, the file `tools/sony_tables.sh` unpacks the
//! long way). Entry 6 of that package, at offset 0x7d70c0, is the root filesystem: AES-128-CBC
//! under the NW-WM1A key with a fixed IV, and under that a run of zlib chunks (`u32` length, then
//! the stream), each inflating to one 4096-byte ext4 block. The tables are contiguous files at
//! blocks 8926 (`ov_127x`), 8968 (`ov_1280`) and 9010 (`ov_dsd_127x`), and the chunks for blocks
//! 8926..=9013 are entry bytes 4135655..4172290 — stored uncompressed in the disk image at
//! `SPAN_AT`. CBC lets a span be decrypted on its own given the 16 bytes before it, so the request
//! starts one block early. Every result is checked against the SHA-256 the player's installer
//! checks, so a changed file on Sony's side fails loudly rather than installing anything else.

use std::fs;
use std::path::Path;

pub const URL: &str = "https://walkman.update.sony.net/fw/pc/WM1/J/NW-WM1_V3_02.dmg";
/// Disk-image offset of the 16 bytes before the span (the CBC chaining block), and the request's
/// length: those 16 plus the span, rounded to whole AES blocks.
const SPAN_AT: u64 = 11_645_442;
const SPAN_LEN: usize = 16 + 36_656;
/// Bytes from the start of the decrypted span to the first chunk's length word.
const FIRST_CHUNK: usize = 7;
const FIRST_BLOCK: usize = 8926;
const CHUNKS: usize = 88;
const BLOCK: usize = 4096;
/// The NW-WM1A package key. (Entries also have a fixed IV, but it only chains an entry's first
/// block; this span starts mid-entry and brings its own.)
const KEY: &[u8; 16] = b"500597cc7292ac15";

/// (file name, first block, size, SHA-256) — the hashes install_cinderhome.sh's `vt_take` checks.
pub const TABLES: [(&str, usize, usize, &str); 3] = [
    (
        "ov_127x.tbl",
        8926,
        84950,
        "b5dd878b0484c43312f3a93c6675d40d546f78ef7494684991eea0aef8651320",
    ),
    (
        "ov_dsd_127x.tbl",
        9010,
        13076,
        "b777b7e2786f952797e82830d5d17565d03a4758555436a376bf6daff5e50ac7",
    ),
    (
        "ov_1280.tbl",
        8968,
        84950,
        "0902981b0b00b5f98eb04d00713f4ddc96f4bb91af76289d8cce15486c30a9c7",
    ),
];

/// Does this curve choice need the tables?
pub fn needed(voltable: &str) -> bool {
    matches!(voltable, "wm1a" | "w1")
}

/// Is `name` already in the drive root, byte for byte Sony's?
pub fn on_drive(target: &Path, name: &str) -> bool {
    let want = TABLES.iter().find(|t| t.0 == name).map(|t| t.3);
    match (fs::read(target.join(name)), want) {
        (Ok(b), Some(w)) => hex(&sha256(&b)) == w,
        _ => false,
    }
}

/// Download the span and return the three tables, each checked.
pub fn fetch() -> Result<Vec<(&'static str, Vec<u8>)>, String> {
    let range = format!(
        "Range: bytes={}-{}\r\n",
        SPAN_AT,
        SPAN_AT + SPAN_LEN as u64 - 1
    );
    let span = crate::release::get(URL, &range, SPAN_LEN)?;
    if span.len() != SPAN_LEN {
        return Err(format!(
            "Sony's server sent {} bytes where {} were asked for",
            span.len(),
            SPAN_LEN
        ));
    }
    extract(&span)
}

/// The tables out of the downloaded bytes: decrypt, inflate the chunks, cut, check.
pub fn extract(span: &[u8]) -> Result<Vec<(&'static str, Vec<u8>)>, String> {
    if !span.len().is_multiple_of(16) || span.len() < 32 {
        return Err("the downloaded span is not whole AES blocks".into());
    }
    let mut prev: [u8; 16] = span[..16].try_into().unwrap();
    let rk = aes128_key_schedule(KEY);
    let mut plain = Vec::with_capacity(span.len() - 16);
    for c in span[16..].chunks_exact(16) {
        let c: [u8; 16] = c.try_into().unwrap();
        let mut p = aes128_decrypt_block(&rk, &c);
        for i in 0..16 {
            p[i] ^= prev[i];
        }
        plain.extend_from_slice(&p);
        prev = c;
    }

    let mut blocks = Vec::with_capacity(CHUNKS * BLOCK);
    let mut pos = FIRST_CHUNK;
    for _ in 0..CHUNKS {
        let n = plain
            .get(pos..pos + 4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize)
            .ok_or("the span ends inside a chunk header")?;
        let z = plain
            .get(pos + 4..pos + 4 + n)
            .ok_or("the span ends inside a chunk")?;
        let out = zlib_inflate(z, BLOCK)?;
        if out.len() != BLOCK {
            return Err(format!(
                "a chunk inflated to {} bytes, not {BLOCK}",
                out.len()
            ));
        }
        blocks.extend_from_slice(&out);
        pos += 4 + n;
    }

    let mut tables = Vec::new();
    for (name, block, size, sha) in TABLES {
        let at = (block - FIRST_BLOCK) * BLOCK;
        let bytes = blocks[at..at + size].to_vec();
        if hex(&sha256(&bytes)) != sha {
            return Err(format!(
                "{name} from Sony's server is not the file Cinder expects"
            ));
        }
        tables.push((name, bytes));
    }
    Ok(tables)
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

// ── AES-128, decryption only (FIPS-197) ────────────────────────────────────────────────────

fn xtime(a: u8) -> u8 {
    (a << 1) ^ if a & 0x80 != 0 { 0x1b } else { 0 }
}

fn gmul(mut a: u8, mut b: u8) -> u8 {
    let mut p = 0;
    while b != 0 {
        if b & 1 != 0 {
            p ^= a;
        }
        a = xtime(a);
        b >>= 1;
    }
    p
}

/// The S-box and its inverse, computed: the multiplicative inverse in GF(2^8), then the affine map.
fn sboxes() -> ([u8; 256], [u8; 256]) {
    let mut s = [0u8; 256];
    let mut inv = [0u8; 256];
    for (x, sx) in s.iter_mut().enumerate() {
        let b = if x == 0 {
            0
        } else {
            (1..=255u8).find(|&y| gmul(x as u8, y) == 1).unwrap()
        };
        *sx = b ^ b.rotate_left(1) ^ b.rotate_left(2) ^ b.rotate_left(3) ^ b.rotate_left(4) ^ 0x63;
        inv[*sx as usize] = x as u8;
    }
    (s, inv)
}

struct RoundKeys {
    w: [u8; 176],
    inv: [u8; 256],
}

fn aes128_key_schedule(key: &[u8; 16]) -> RoundKeys {
    let (s, inv) = sboxes();
    let mut w = [0u8; 176];
    w[..16].copy_from_slice(key);
    let mut rcon = 1u8;
    for i in 4..44 {
        let mut t: [u8; 4] = w[4 * i - 4..4 * i].try_into().unwrap();
        if i % 4 == 0 {
            t = [
                s[t[1] as usize] ^ rcon,
                s[t[2] as usize],
                s[t[3] as usize],
                s[t[0] as usize],
            ];
            rcon = xtime(rcon);
        }
        for j in 0..4 {
            w[4 * i + j] = w[4 * i - 16 + j] ^ t[j];
        }
    }
    RoundKeys { w, inv }
}

fn aes128_decrypt_block(rk: &RoundKeys, input: &[u8; 16]) -> [u8; 16] {
    let add = |st: &mut [u8; 16], r: usize| {
        for (b, k) in st.iter_mut().zip(&rk.w[16 * r..16 * r + 16]) {
            *b ^= k;
        }
    };
    // Byte i of the state is row i % 4, column i / 4. Inverse ShiftRows moves row r right by r.
    let inv_shift_sub = |st: &mut [u8; 16]| {
        let o = *st;
        for c in 0..4 {
            for r in 0..4 {
                st[r + 4 * c] = rk.inv[o[r + 4 * ((c + 4 - r) % 4)] as usize];
            }
        }
    };
    let mut st = *input;
    add(&mut st, 10);
    for round in (1..10).rev() {
        inv_shift_sub(&mut st);
        add(&mut st, round);
        for c in 0..4 {
            let a = [st[4 * c], st[4 * c + 1], st[4 * c + 2], st[4 * c + 3]];
            st[4 * c] = gmul(a[0], 14) ^ gmul(a[1], 11) ^ gmul(a[2], 13) ^ gmul(a[3], 9);
            st[4 * c + 1] = gmul(a[0], 9) ^ gmul(a[1], 14) ^ gmul(a[2], 11) ^ gmul(a[3], 13);
            st[4 * c + 2] = gmul(a[0], 13) ^ gmul(a[1], 9) ^ gmul(a[2], 14) ^ gmul(a[3], 11);
            st[4 * c + 3] = gmul(a[0], 11) ^ gmul(a[1], 13) ^ gmul(a[2], 9) ^ gmul(a[3], 14);
        }
    }
    inv_shift_sub(&mut st);
    add(&mut st, 0);
    st
}

// ── zlib inflate (RFC 1950/1951) ───────────────────────────────────────────────────────────

struct Bits<'a> {
    d: &'a [u8],
    pos: usize,
    bit: u32,
    nbits: u32,
}

impl Bits<'_> {
    fn need(&mut self, n: u32) -> Result<u32, String> {
        while self.nbits < n {
            let b = *self.d.get(self.pos).ok_or("compressed data ends early")?;
            self.pos += 1;
            self.bit |= (b as u32) << self.nbits;
            self.nbits += 8;
        }
        let v = self.bit & ((1u32 << n) - 1);
        self.bit = if n == 32 { 0 } else { self.bit >> n };
        self.nbits -= n;
        Ok(v)
    }
}

/// Canonical Huffman code as counts per length and symbols in code order.
struct Huff {
    count: [u16; 16],
    sym: Vec<u16>,
}

fn huff(lengths: &[u8]) -> Huff {
    let mut count = [0u16; 16];
    for &l in lengths {
        count[l as usize] += 1;
    }
    count[0] = 0;
    let mut offs = [0u16; 16];
    for l in 1..16 {
        offs[l] = offs[l - 1] + count[l - 1];
    }
    let mut sym = vec![0u16; lengths.len()];
    for (s, &l) in lengths.iter().enumerate() {
        if l != 0 {
            sym[offs[l as usize] as usize] = s as u16;
            offs[l as usize] += 1;
        }
    }
    Huff { count, sym }
}

fn decode(b: &mut Bits, h: &Huff) -> Result<u16, String> {
    let (mut code, mut first, mut index) = (0i32, 0i32, 0i32);
    for len in 1..16 {
        code |= b.need(1)? as i32;
        let count = h.count[len] as i32;
        if code - first < count {
            return Ok(h.sym[(index + code - first) as usize]);
        }
        index += count;
        first = (first + count) << 1;
        code <<= 1;
    }
    Err("bad Huffman code".into())
}

const LBASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LEXT: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DBASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DEXT: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

/// Inflate one zlib stream into at most `max` bytes. The checksum is not verified: every caller
/// checks a SHA-256 of what it builds from the output.
fn zlib_inflate(z: &[u8], max: usize) -> Result<Vec<u8>, String> {
    if z.len() < 2
        || z[0] & 0x0f != 8
        || (u16::from(z[0]) << 8 | u16::from(z[1])) % 31 != 0
        || z[1] & 0x20 != 0
    {
        return Err("not a zlib stream".into());
    }
    let mut b = Bits {
        d: &z[2..],
        pos: 0,
        bit: 0,
        nbits: 0,
    };
    let mut out: Vec<u8> = Vec::new();
    loop {
        let last = b.need(1)?;
        match b.need(2)? {
            0 => {
                b.bit = 0;
                b.nbits = 0;
                let h =
                    b.d.get(b.pos..b.pos + 4)
                        .ok_or("stored block header cut short")?;
                let len = u16::from_le_bytes([h[0], h[1]]) as usize;
                if len != !u16::from_le_bytes([h[2], h[3]]) as usize {
                    return Err("stored block length check failed".into());
                }
                b.pos += 4;
                out.extend_from_slice(
                    b.d.get(b.pos..b.pos + len)
                        .ok_or("stored block cut short")?,
                );
                b.pos += len;
            }
            t @ (1 | 2) => {
                let (lit, dist) = if t == 1 {
                    let mut l = [8u8; 288];
                    l[144..256].fill(9);
                    l[256..280].fill(7);
                    (huff(&l), huff(&[5u8; 30]))
                } else {
                    let nlen = b.need(5)? as usize + 257;
                    let ndist = b.need(5)? as usize + 1;
                    let ncode = b.need(4)? as usize + 4;
                    const ORDER: [usize; 19] = [
                        16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
                    ];
                    let mut cl = [0u8; 19];
                    for &i in &ORDER[..ncode] {
                        cl[i] = b.need(3)? as u8;
                    }
                    let ch = huff(&cl);
                    let mut lens = vec![0u8; nlen + ndist];
                    let mut i = 0;
                    while i < nlen + ndist {
                        let s = decode(&mut b, &ch)?;
                        let (val, rep) = match s {
                            0..=15 => (s as u8, 1),
                            16 => (
                                *lens
                                    .get(i.wrapping_sub(1))
                                    .ok_or("repeat with nothing before it")?,
                                3 + b.need(2)? as usize,
                            ),
                            17 => (0, 3 + b.need(3)? as usize),
                            _ => (0, 11 + b.need(7)? as usize),
                        };
                        if i + rep > lens.len() {
                            return Err("code lengths overrun".into());
                        }
                        lens[i..i + rep].fill(val);
                        i += rep;
                    }
                    (huff(&lens[..nlen]), huff(&lens[nlen..]))
                };
                loop {
                    let s = decode(&mut b, &lit)? as usize;
                    if s < 256 {
                        out.push(s as u8);
                    } else if s == 256 {
                        break;
                    } else {
                        let s = s - 257;
                        if s >= 29 {
                            return Err("bad length code".into());
                        }
                        let len = LBASE[s] as usize + b.need(LEXT[s] as u32)? as usize;
                        let d = decode(&mut b, &dist)? as usize;
                        if d >= 30 {
                            return Err("bad distance code".into());
                        }
                        let back = DBASE[d] as usize + b.need(DEXT[d] as u32)? as usize;
                        if back > out.len() {
                            return Err("distance before the start".into());
                        }
                        for _ in 0..len {
                            out.push(out[out.len() - back]);
                        }
                    }
                    if out.len() > max {
                        return Err("inflates past the expected size".into());
                    }
                }
            }
            _ => return Err("reserved block type".into()),
        }
        if out.len() > max {
            return Err("inflates past the expected size".into());
        }
        if last == 1 {
            return Ok(out);
        }
    }
}

// ── SHA-256 (FIPS 180-4) ───────────────────────────────────────────────────────────────────

pub fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for block in msg.chunks_exact(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes(block[4 * i..4 * i + 4].try_into().unwrap());
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let mut v = h;
        for i in 0..64 {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let t1 = v[7]
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(maj);
            v = [
                t1.wrapping_add(t2),
                v[0],
                v[1],
                v[2],
                v[3].wrapping_add(t1),
                v[4],
                v[5],
                v[6],
            ];
        }
        for i in 0..8 {
            h[i] = h[i].wrapping_add(v[i]);
        }
    }
    let mut out = [0u8; 32];
    for i in 0..8 {
        out[4 * i..4 * i + 4].copy_from_slice(&h[i].to_be_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unhex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn aes128_fips197_appendix_c1() {
        let key: [u8; 16] = unhex("000102030405060708090a0b0c0d0e0f")
            .try_into()
            .unwrap();
        let ct: [u8; 16] = unhex("69c4e0d86a7b0430d8cdb78070b4c55a")
            .try_into()
            .unwrap();
        assert_eq!(
            hex(&aes128_decrypt_block(&aes128_key_schedule(&key), &ct)),
            "00112233445566778899aabbccddeeff"
        );
    }

    #[test]
    fn sha256_known_answers() {
        assert_eq!(
            hex(&sha256(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hex(&sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let long = vec![b'a'; 1000];
        assert_eq!(
            hex(&sha256(&long)),
            "41edece42d63e8d9bf515a9ba6932e1c20cbc9f5a5d134645adb5db1b9737ea3"
        );
    }

    // Fixtures made with Python's zlib: a stored block, a fixed-Huffman block and a dynamic one.
    #[test]
    fn inflate_stored_fixed_and_dynamic() {
        // zlib.compress(b"hello", 0)
        assert_eq!(
            zlib_inflate(&unhex("7801010500faff68656c6c6f062c0215"), 64).unwrap(),
            b"hello"
        );
        // zlib.compress(b"hello hello hello hello", 9)
        assert_eq!(
            zlib_inflate(&unhex("78dacb48cdc9c957c8402701680308b1"), 64).unwrap(),
            b"hello hello hello hello"
        );
        // zlib.compress(d, 9), d as built below
        let mut want: Vec<u8> = Vec::new();
        for i in 0..400u32 {
            want.extend(std::iter::repeat(97 + (i * i % 7) as u8).take(1 + (i % 5) as usize));
        }
        want.extend(b"the quick brown fox jumps over the lazy dog ".repeat(6));
        let z = unhex(concat!(
            "78daed8d4b0e80300844afc2d518c5bf56abf5777a2dd15ec09d611624c09b3c0644247b230033037ad5fb3d",
            "e3ca3148a47ea1a4a4be92fcf42109d4954d64221399e89b68a984a650672dc1bb6da0c2edd4847e9cc9ade229",
            "be3b3e0fca5d497f662fa7f430bf"
        ));
        assert_eq!(zlib_inflate(&z, 4096).unwrap(), want);
    }

    #[test]
    fn inflate_refuses_more_than_asked() {
        assert!(zlib_inflate(&unhex("78dacb48cdc9c957c8402701680308b1"), 10).is_err());
    }

    #[test]
    fn only_the_two_curves_that_need_them() {
        assert!(needed("wm1a") && needed("w1"));
        assert!(!needed("stock") && !needed("region"));
    }

    /// The real thing, over the network: `cargo test -- --ignored sony`.
    #[test]
    #[ignore]
    fn sony_serves_the_three_tables() {
        let t = fetch().unwrap();
        assert_eq!(t.len(), 3);
    }
}
