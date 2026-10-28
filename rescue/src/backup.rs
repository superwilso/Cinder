//! What a backup is: the A50-series partition table, a check that an image really has that layout,
//! and the manifest written beside every dump (chip, eMMC CID, range, SHA-256 per partition).
//!
//! The manifest's hashes are taken from the bytes as they came off the USB link, so `verify`
//! re-reading the file proves the file on disk is what the player sent.

use crate::sha256::Sha256;
use std::io::Read;

/// The eMMC user area of an NW-A50-series player, from its `/proc/dumchar_info` (2026-10-26, A55).
/// `preloader` is not here: it lives in the boot0 hardware partition, outside the user area, and
/// nothing in this tool reads or writes it.
pub const TABLE: &[(&str, u64, u64)] = &[
    ("mbr", 0x0, 0x80000),
    ("ebr1", 0x80000, 0x80000),
    ("ebr2", 0x100000, 0x80000),
    ("ebr3", 0x180000, 0x80000),
    ("ebr4", 0x200000, 0x80000),
    ("ebr5", 0x280000, 0x80000),
    ("ebr6", 0x300000, 0x80000),
    ("ebr7", 0x380000, 0x80000),
    ("ebr8", 0x400000, 0x80000),
    ("ebr9", 0x480000, 0x80000),
    ("pro_info", 0x500000, 0x300000),
    ("nvram", 0x800000, 0x500000),
    ("protect_f", 0xd00000, 0xa00000),
    ("protect_s", 0x1700000, 0xa00000),
    ("seccfg", 0x2100000, 0x20000),
    ("uboot", 0x2120000, 0x60000),
    ("bootimg", 0x2180000, 0x1000000),
    ("recovery", 0x3180000, 0x1000000),
    ("sec_ro", 0x4180000, 0x600000),
    ("misc", 0x4780000, 0x80000),
    ("logo", 0x4800000, 0x300000),
    ("expdb", 0x4b00000, 0xa00000),
    ("tee1", 0x5500000, 0x500000),
    ("tee2", 0x5a00000, 0x500000),
    ("kb", 0x5f00000, 0x100000),
    ("dkb", 0x6000000, 0x100000),
    ("xhrome", 0x6100000, 0x100000),
    ("android", 0x6200000, 0x32000000),
    ("cache", 0x38200000, 0x1c00000),
    ("cm4", 0x39e00000, 0x80000),
    ("nvp", 0x39e80000, 0xf00000),
    ("var", 0x3ad80000, 0x6e00000),
    ("db", 0x41b80000, 0x6400000),
    ("option1", 0x47f80000, 0x800000),
    ("option2", 0x48780000, 0x8000000),
    ("option3", 0x50780000, 0x40000000),
    ("usrdata", 0x90780000, 0x2800000),
    ("contents", 0x92f80000, 0xdfc880000),
];

/// Sony's serial number: NUL-padded ASCII at `nvp` + 0x1c000 (the A55's own dump, 2026-10-26).
/// What proves a wbrt backup, which records no eMMC ID, belongs to the player it goes back to.
pub const SERIAL_AT: u64 = 0x39e9c000;

/// The serial in the 16 bytes at `SERIAL_AT`, if they hold one.
pub fn serial(b: &[u8]) -> Option<String> {
    let s = b.get(..16)?.split(|&c| c == 0).next()?;
    (!s.is_empty() && s.iter().all(u8::is_ascii_alphanumeric))
        .then(|| String::from_utf8_lossy(s).into_owned())
}

/// Table index of p2 (`pro_info`); p2..p29 follow in table order.
const FIRST_NUMBERED: usize = 10;

#[derive(Debug, PartialEq)]
pub struct Region {
    pub name: &'static str,
    pub start: u64,
    pub end: u64,
}

/// The table clipped to `[addr, addr + len)`, dropping what falls outside.
pub fn regions(addr: u64, len: u64) -> Vec<Region> {
    TABLE
        .iter()
        .map(|&(name, s, n)| Region {
            name,
            start: s.max(addr),
            end: (s + n).min(addr + len),
        })
        .filter(|r| r.start < r.end)
        .collect()
}

/// Check the MBR and MediaTek EBR chain in the first 5 MiB of an image against `TABLE`.
/// The MBR holds p2-p4 at absolute sectors and points at the first EBR; each EBR holds three
/// partitions relative to itself, and its fourth entry links the next EBR relative to the first.
pub fn check_layout(head: &[u8]) -> Result<(), String> {
    let sector = |off: u64| -> Result<&[u8], String> {
        let s = head
            .get(off as usize..off as usize + 512)
            .ok_or("image too short for its partition table")?;
        if s[510..] != [0x55, 0xaa] {
            return Err(format!("no partition table signature at {off:#x}"));
        }
        Ok(s)
    };
    let entry = |s: &[u8], i: usize| {
        let e = &s[446 + 16 * i..];
        (
            e[4],
            u32::from_le_bytes([e[8], e[9], e[10], e[11]]) as u64,
            u32::from_le_bytes([e[12], e[13], e[14], e[15]]) as u64,
        )
    };
    let mbr = sector(0)?;
    let mut found = vec![];
    for i in 1..4 {
        let (_, s, n) = entry(mbr, i);
        found.push((s * 512, n * 512));
    }
    let (t, first_ebr, _) = entry(mbr, 0);
    if t != 5 {
        return Err("the MBR does not point at an extended partition".into());
    }
    let mut ebr = first_ebr;
    for _ in 0..16 {
        let s = sector(ebr * 512)?;
        for i in 0..3 {
            let (t, st, n) = entry(s, i);
            if t == 0x83 {
                found.push(((ebr + st) * 512, n * 512));
            }
        }
        let (t, next, _) = entry(s, 3);
        if t != 5 {
            break;
        }
        ebr = first_ebr + next;
    }
    let want = &TABLE[FIRST_NUMBERED..];
    for (i, &(name, s, n)) in want.iter().enumerate() {
        let last = i + 1 == want.len(); // the last partition's size is "rest of the disk"
        match found.get(i) {
            Some(&(fs, fn_)) if fs == s && (last || fn_ == n) => {}
            Some(&(fs, fn_)) => {
                return Err(format!(
                    "p{} ({name}) is at {fs:#x}+{fn_:#x}, an A50-series player has {s:#x}+{n:#x}",
                    i + 2
                ))
            }
            None => {
                return Err(format!(
                    "the partition table stops before p{} ({name})",
                    i + 2
                ))
            }
        }
    }
    Ok(())
}

/// SHA-256 per region, fed with data at absolute eMMC positions in any piece sizes.
pub struct Hashes {
    pub regions: Vec<Region>,
    sums: Vec<Sha256>,
}

impl Hashes {
    pub fn new(addr: u64, len: u64) -> Self {
        let regions = regions(addr, len);
        let sums = regions.iter().map(|_| Sha256::new()).collect();
        Hashes { regions, sums }
    }

    pub fn feed(&mut self, pos: u64, data: &[u8]) {
        let end = pos + data.len() as u64;
        for (r, s) in self.regions.iter().zip(&mut self.sums) {
            let (a, b) = (r.start.max(pos), r.end.min(end));
            if a < b {
                s.update(&data[(a - pos) as usize..(b - pos) as usize]);
            }
        }
    }

    pub fn finish(self) -> Vec<(Region, String)> {
        self.regions
            .into_iter()
            .zip(self.sums.into_iter().map(Sha256::hex))
            .collect()
    }
}

pub struct Manifest {
    pub header: Vec<String>,
    pub addr: u64,
    pub len: u64,
    pub sums: Vec<(String, u64, u64, String)>,
}

pub fn write_manifest(
    path: &str,
    header: &[String],
    addr: u64,
    len: u64,
    sums: &[(Region, String)],
) -> Result<(), String> {
    let mut s = format!("cinder-rescue backup {}\n", env!("CARGO_PKG_VERSION"));
    for h in header {
        s += &format!("{h}\n");
    }
    s += &format!("range {addr:#x} {len:#x}\n");
    for (r, h) in sums {
        s += &format!(
            "region {} {:#x} {:#x} {h}\n",
            r.name,
            r.start,
            r.end - r.start
        );
    }
    std::fs::write(path, s).map_err(|e| format!("{path}: {e}"))
}

pub fn read_manifest(path: &str) -> Result<Manifest, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let num = |s: &str| {
        u64::from_str_radix(s.trim_start_matches("0x"), 16)
            .map_err(|_| format!("{path}: bad number {s}"))
    };
    let mut m = Manifest {
        header: vec![],
        addr: 0,
        len: 0,
        sums: vec![],
    };
    let mut range = false;
    for line in text.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        match f.as_slice() {
            ["range", a, l] => {
                (m.addr, m.len) = (num(a)?, num(l)?);
                range = true;
            }
            ["region", n, s, l, h] => m
                .sums
                .push((n.to_string(), num(s)?, num(l)?, h.to_string())),
            _ => m.header.push(line.to_string()),
        }
    }
    if !range {
        return Err(format!(
            "{path}: not a cinder-rescue manifest (no range line)"
        ));
    }
    Ok(m)
}

/// Hash a backup file whose first byte is eMMC position `addr`. Calls `progress` with bytes read.
pub fn hash_file(
    path: &str,
    addr: u64,
    len: u64,
    mut progress: impl FnMut(u64),
) -> Result<Vec<(Region, String)>, String> {
    let mut f = std::fs::File::open(path).map_err(|e| format!("{path}: {e}"))?;
    let size = f.metadata().map_err(|e| format!("{path}: {e}"))?.len();
    if size != len {
        return Err(format!(
            "{path} is {size} bytes, the backup should be {len}"
        ));
    }
    let mut h = Hashes::new(addr, len);
    let mut buf = vec![0u8; 1 << 22];
    let mut pos = 0u64;
    while pos < len {
        let n = f.read(&mut buf).map_err(|e| format!("{path}: {e}"))?;
        if n == 0 {
            return Err(format!("{path} ended early at byte {pos}"));
        }
        h.feed(addr + pos, &buf[..n]);
        pos += n as u64;
        progress(pos);
    }
    Ok(h.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The MBR and nine EBR sectors' entries from the A55's own dump (2026-10-26):
    /// (type, start sector, sector count) x 4 per sector, sectors 0x80000 apart.
    const REAL: [[(u8, u32, u32); 4]; 10] = [
        [
            (5, 0x400, 0xffffffff),
            (0x83, 0x2800, 0x1800),
            (0x83, 0x4000, 0x2800),
            (0x83, 0x6800, 0x5000),
        ],
        [
            (0x83, 0xb400, 0x5000),
            (0x83, 0x10400, 0x100),
            (0x83, 0x10500, 0x300),
            (5, 0x400, 0xffffffff),
        ],
        [
            (0x83, 0x10400, 0x8000),
            (0x83, 0x18400, 0x8000),
            (0x83, 0x20400, 0x3000),
            (5, 0x800, 0xffffffff),
        ],
        [
            (0x83, 0x23000, 0x400),
            (0x83, 0x23400, 0x1800),
            (0x83, 0x24c00, 0x5000),
            (5, 0xc00, 0xffffffff),
        ],
        [
            (0x83, 0x29800, 0x2800),
            (0x83, 0x2c000, 0x2800),
            (0x83, 0x2e800, 0x800),
            (5, 0x1000, 0xffffffff),
        ],
        [
            (0x83, 0x2ec00, 0x800),
            (0x83, 0x2f400, 0x800),
            (0x83, 0x2fc00, 0x190000),
            (5, 0x1400, 0xffffffff),
        ],
        [
            (0x83, 0x1bf800, 0xe000),
            (0x83, 0x1cd800, 0x400),
            (0x83, 0x1cdc00, 0x7800),
            (5, 0x1800, 0xffffffff),
        ],
        [
            (0x83, 0x1d5000, 0x37000),
            (0x83, 0x20c000, 0x32000),
            (0x83, 0x23e000, 0x4000),
            (5, 0x1c00, 0xffffffff),
        ],
        [
            (0x83, 0x241c00, 0x40000),
            (0x83, 0x281c00, 0x200000),
            (0x83, 0x481c00, 0x14000),
            (5, 0x2000, 0xffffffff),
        ],
        [
            (0x83, 0x495800, 0x20000000),
            (0x83, 0xffffdc00, 0x3ff),
            (0, 0, 0),
            (0, 0, 0),
        ],
    ];

    fn head() -> Vec<u8> {
        let mut b = vec![0u8; 0x500000];
        for (k, ents) in REAL.iter().enumerate() {
            let s = &mut b[k * 0x80000..k * 0x80000 + 512];
            for (i, &(t, st, n)) in ents.iter().enumerate() {
                let e = &mut s[446 + 16 * i..446 + 16 * i + 16];
                e[4] = t;
                e[8..12].copy_from_slice(&st.to_le_bytes());
                e[12..16].copy_from_slice(&n.to_le_bytes());
            }
            s[510..].copy_from_slice(&[0x55, 0xaa]);
        }
        b
    }

    #[test]
    fn real_layout_matches_the_table_and_a_moved_partition_does_not() {
        let mut b = head();
        assert_eq!(check_layout(&b), Ok(()));
        b[0x80000 + 446 + 8] ^= 1; // nudge p5 (protect_s) by a sector
        assert!(check_layout(&b).unwrap_err().contains("p5 (protect_s)"));
    }

    #[test]
    fn serial_is_the_ascii_before_the_padding() {
        assert_eq!(
            serial(b"5194859\0\0\0\0\0\0\0\0\0").as_deref(),
            Some("5194859")
        );
        assert_eq!(serial(&[0; 16]), None);
        assert_eq!(serial(&[0xff; 16]), None);
    }

    #[test]
    fn hashes_split_at_partition_edges_whatever_the_piece_size() {
        let data: Vec<u8> = (0..0x600000u32).map(|i| (i % 251) as u8).collect();
        let whole = {
            let mut h = Hashes::new(0, data.len() as u64);
            h.feed(0, &data);
            h.finish()
        };
        let mut h = Hashes::new(0, data.len() as u64);
        for (i, c) in data.chunks(0x12345).enumerate() {
            h.feed(i as u64 * 0x12345, c);
        }
        assert_eq!(h.finish(), whole);
        // mbr, ebr1-9, then pro_info clipped to the 6 MiB range.
        assert_eq!(whole.len(), 11);
        assert_eq!(
            whole[10].0,
            Region {
                name: "pro_info",
                start: 0x500000,
                end: 0x600000
            }
        );
    }
}
