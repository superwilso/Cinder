//! MediaTek download mode, read side only: the preloader handshake, loading the two-stage Download
//! Agent (DA) into the player's RAM, and the DA's eMMC read command.
//!
//! Protocol facts (command bytes, field order, checksums, the MT8590 quirk sequence) come from
//! unknown321's `mediatek_flash_tool`, a fork of Saleem Rashid's: MIT, Copyright (c) 2020 Saleem
//! Rashid, 2025 unknown321. Notice in `LICENSE-mediatek_flash_tool`. Everything on the wire is
//! big-endian; the DA file's own header is little-endian.
//!
//! `write_user` is the one function here that changes the eMMC. Only `restore` in main.rs calls
//! it, after checking the backup, the player's identity and its partition layout.

/// Byte stream to the device. `read` fills the whole buffer or fails.
pub trait Link {
    fn read(&mut self, buf: &mut [u8]) -> Result<(), String>;
    fn write(&mut self, buf: &[u8]) -> Result<(), String>;
    /// Drop anything already received but not yet read.
    fn flush(&mut self);

    fn r8(&mut self) -> Result<u8, String> {
        let mut b = [0; 1];
        self.read(&mut b)?;
        Ok(b[0])
    }
    fn r16(&mut self) -> Result<u16, String> {
        let mut b = [0; 2];
        self.read(&mut b)?;
        Ok(u16::from_be_bytes(b))
    }
    fn r32(&mut self) -> Result<u32, String> {
        let mut b = [0; 4];
        self.read(&mut b)?;
        Ok(u32::from_be_bytes(b))
    }
    fn skip(&mut self, n: usize) -> Result<(), String> {
        self.read(&mut vec![0; n])
    }
    fn w8(&mut self, v: u8) -> Result<(), String> {
        self.write(&[v])
    }
    fn w16(&mut self, v: u16) -> Result<(), String> {
        self.write(&v.to_be_bytes())
    }
    fn w32(&mut self, v: u32) -> Result<(), String> {
        self.write(&v.to_be_bytes())
    }
    fn w64(&mut self, v: u64) -> Result<(), String> {
        self.write(&v.to_be_bytes())
    }
    /// The preloader echoes every command byte and argument back.
    fn echo8(&mut self, v: u8) -> Result<(), String> {
        self.w8(v)?;
        match self.r8()? {
            r if r == v => Ok(()),
            r => Err(format!("preloader echoed {r:#04x} for {v:#04x}")),
        }
    }
    fn echo32(&mut self, v: u32) -> Result<(), String> {
        self.w32(v)?;
        match self.r32()? {
            r if r == v => Ok(()),
            r => Err(format!("preloader echoed {r:#x} for {v:#x}")),
        }
    }
}

pub const ACK: u8 = 0x5a;
const SYNC_CHAR: u8 = 0xc0;
const SOC_OK: u8 = 0xc1;
const CONT_CHAR: u8 = 0x69;
const NAND_NOT_FOUND: u32 = 0xbc4;
const STORAGE_SDMMC: u8 = 2;
const PART_USER: u8 = 8;
const HOST_OS_LINUX: u8 = 12;
/// Bytes per checksummed packet the DA sends on a read.
pub const READ_CHUNK: usize = 0x10_0000;

// ---- the DA file ----

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Region {
    pub offset: u32,
    pub len: u32,
    pub start_addr: u32,
    pub sig_offset: u32,
    pub sig_len: u32,
}

#[derive(Debug)]
pub struct DaEntry {
    pub hw_code: u16,
    pub hw_ver: u16,
    pub sw_ver: u16,
    pub entry_region: usize,
    pub regions: Vec<Region>,
}

const DA_HEADER: usize = 108;
const DA_ENTRY: usize = 220;

/// The DA file's description and its per-chip entries.
pub fn parse_da(b: &[u8]) -> Result<(String, Vec<DaEntry>), String> {
    let u16le = |o: usize| u16::from_le_bytes([b[o], b[o + 1]]);
    let u32le = |o: usize| u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
    if b.len() < DA_HEADER || u32le(96) != 4 || u32le(100) != 0x2266_8899 {
        return Err("not a MediaTek Download Agent file (header magic)".into());
    }
    let count = u32le(104) as usize;
    if b.len() < DA_HEADER + count * DA_ENTRY {
        return Err(format!("Download Agent file is cut short ({count} entries)"));
    }
    let desc = String::from_utf8_lossy(&b[32..96]).trim_end_matches('\0').to_string();
    let mut out = Vec::new();
    for i in 0..count {
        let e = DA_HEADER + i * DA_ENTRY;
        if u16le(e) != 0xdada {
            return Err(format!("Download Agent entry {i} has a bad magic"));
        }
        let n = (u16le(e + 18) as usize).min(10);
        let regions = (0..n)
            .map(|r| {
                let o = e + 20 + r * 20;
                Region {
                    offset: u32le(o),
                    len: u32le(o + 4),
                    start_addr: u32le(o + 8),
                    sig_offset: u32le(o + 12),
                    sig_len: u32le(o + 16),
                }
            })
            .collect();
        out.push(DaEntry {
            hw_code: u16le(e + 2),
            hw_ver: u16le(e + 6),
            sw_ver: u16le(e + 8),
            entry_region: u16le(e + 16) as usize,
            regions,
        });
    }
    Ok((desc, out))
}

/// The two stages for this chip: the first signed region from the entry region on, and the next.
pub fn pick_stages(entries: &[DaEntry], hw_code: u16, hw_ver: u16, sw_ver: u16) -> Result<(Region, Region), String> {
    let e = entries
        .iter()
        .find(|e| e.hw_code == hw_code && e.hw_ver <= hw_ver && e.sw_ver <= sw_ver)
        .ok_or_else(|| format!("the Download Agent file has nothing for chip {hw_code:#06x}"))?;
    let i = (e.entry_region..e.regions.len().saturating_sub(1))
        .find(|&i| e.regions[i].sig_len > 0)
        .ok_or("the Download Agent entry has no loadable stage 1")?;
    let (s1, s2) = (e.regions[i], e.regions[i + 1]);
    for s in [s1, s2] {
        if s.sig_offset.checked_add(s.sig_len) != Some(s.len) {
            return Err("a Download Agent stage's signature is not at its end".into());
        }
    }
    Ok((s1, s2))
}

fn slice(da: &[u8], r: Region) -> Result<&[u8], String> {
    da.get(r.offset as usize..r.offset as usize + r.len as usize)
        .ok_or_else(|| "a Download Agent stage runs past the end of the file".into())
}

// ---- preloader ----

pub struct Chip {
    pub hw_code: u16,
    pub hw_ver: u16,
    pub sw_ver: u16,
}

pub fn handshake(l: &mut impl Link) -> Result<(), String> {
    const START: [u8; 4] = [0xa0, 0x0a, 0x50, 0x05];
    let mut i = 0;
    for _ in 0..200 {
        if i == START.len() {
            return Ok(());
        }
        l.flush();
        l.w8(START[i])?;
        i = if l.r8()? == !START[i] { i + 1 } else { 0 };
    }
    if i == START.len() { Ok(()) } else { Err("the preloader never answered the start sequence".into()) }
}

fn status(l: &mut impl Link, what: &str) -> Result<(), String> {
    match l.r16()? {
        0 => Ok(()),
        s => Err(format!("preloader refused {what}: status {s:#06x}")),
    }
}

pub fn chip(l: &mut impl Link) -> Result<Chip, String> {
    l.echo8(0xfd)?;
    let hw_code = l.r16()?;
    status(l, "GET_HW_CODE")?;
    l.echo8(0xfc)?;
    let _subcode = l.r16()?;
    let hw_ver = l.r16()?;
    let sw_ver = l.r16()?;
    status(l, "GET_HW_SW_VER")?;
    Ok(Chip { hw_code, hw_ver, sw_ver })
}

/// Everything from a synced preloader to a running DA stage 2. Returns the eMMC CID.
pub fn load_da(l: &mut impl Link, da: &[u8], s1: Region, s2: Region) -> Result<[u32; 4], String> {
    l.echo8(0xd8)?; // GET_TARGET_CONFIG
    let _cfg = l.r32()?;
    status(l, "GET_TARGET_CONFIG")?;

    // Watchdog off: WRITE32 0x22000064 to 0x10007000.
    l.echo8(0xd4)?;
    l.echo32(0x1000_7000)?;
    l.echo32(1)?;
    status(l, "WRITE32")?;
    l.echo32(0x2200_0064)?;
    status(l, "WRITE32 data")?;

    // The MT8590 preloader wants these queries before SEND_DA (flash tool's handle_state_preloader).
    l.echo8(0xd8)?;
    l.skip(6)?;
    for cmd in [0xfe, 0xff, 0xfc] {
        l.w8(cmd)?;
        l.r8()?;
    }
    std::thread::sleep(std::time::Duration::from_millis(50));
    l.skip(8)?;
    l.w8(0xfe)?;
    l.r8()?;

    // SEND_DA stage 1: address, length, signature length, then the bytes; XOR-of-16-bit checksum.
    let body = slice(da, s1)?;
    l.echo8(0xd7)?;
    l.echo32(s1.start_addr)?;
    l.echo32(s1.len)?;
    l.echo32(s1.sig_len)?;
    status(l, "SEND_DA")?;
    for c in body.chunks(0x400) {
        l.write(c)?;
    }
    let sum = xor16(body);
    let dev_sum = l.r16()?;
    status(l, "SEND_DA data")?;
    if dev_sum != sum {
        return Err(format!("stage 1 checksum mismatch: device {dev_sum:#06x}, ours {sum:#06x}"));
    }
    l.echo8(0xd5)?; // JUMP_DA
    l.echo32(s1.start_addr)?;
    status(l, "JUMP_DA")?;

    // Stage 1 sync: NAND result and IDs, eMMC result and CID.
    if l.r8()? != SYNC_CHAR {
        return Err("the Download Agent did not start (no sync byte)".into());
    }
    let nand = l.r32()?;
    let ids = l.r16()?;
    l.skip(2 * ids as usize)?;
    let emmc = l.r32()?;
    let cid = [l.r32()?, l.r32()?, l.r32()?, l.r32()?];
    l.w8(ACK)?;
    l.skip(3)?; // DA version
    if nand != NAND_NOT_FOUND || emmc != 0 {
        return Err(format!("storage init failed: NAND {nand:#x}, eMMC {emmc:#x}"));
    }

    // Stage 2: device config, then the image in 4 KiB packets, each ACKed.
    for b in [&[0xffu8, 1][..], &[0, 8, 0], &[0x70, 0x07, 0xff, 0xff], &[1], &[0, 0, 0, 0], &[2, 1, 2, 0]] {
        l.write(b)?;
    }
    std::thread::sleep(std::time::Duration::from_millis(350));
    l.r32()?;
    let body = slice(da, s2)?;
    l.w32(s2.start_addr)?;
    l.w32(s2.len)?;
    l.w32(0x1000)?;
    ack(l, "stage 2 header")?;
    for c in body.chunks(0x1000) {
        l.write(c)?;
        ack(l, "stage 2 packet")?;
    }
    std::thread::sleep(std::time::Duration::from_millis(500));
    l.w8(ACK)?;
    l.r8()?;
    for n in [0x1c, 0x11, 0xe, 0x9, 0x5c, 0x1c, 0x26] {
        l.skip(n)?; // flash reports
    }
    let mut pi = [0u8; 10];
    l.read(&mut pi)?;
    if pi[0] != ACK && u32::from_be_bytes([pi[1], pi[2], pi[3], pi[4]]) == ACK as u32 {
        l.skip(3)?;
        if l.r8()? != SOC_OK {
            return Err("the Download Agent reported the chip not OK".into());
        }
    }

    // USB_CHECK_STATUS also switches the DA to high speed.
    l.w8(0x72)?;
    ack(l, "USB_CHECK_STATUS")?;
    match l.r8()? {
        1 => Ok(cid),
        s => Err(format!("Download Agent USB status {s:#04x}, expected 1")),
    }
}

fn ack(l: &mut impl Link, what: &str) -> Result<(), String> {
    match l.r8()? {
        ACK => Ok(()),
        r => Err(format!("Download Agent refused {what}: {r:#04x}")),
    }
}

pub fn xor16(b: &[u8]) -> u16 {
    let mut s = 0u16;
    for w in b.chunks(2) {
        s ^= w[0] as u16 | (*w.get(1).unwrap_or(&0) as u16) << 8;
    }
    s
}

// ---- DA stage 2 ----

/// Read `len` bytes of the eMMC user area from `addr`, handing each verified chunk to `sink`.
pub fn read_user(
    l: &mut impl Link,
    addr: u64,
    len: u64,
    mut sink: impl FnMut(&[u8]) -> Result<(), String>,
) -> Result<(), String> {
    switch_to_user(l)?;

    l.w8(0xd6)?; // READ
    l.w8(HOST_OS_LINUX)?;
    l.w8(STORAGE_SDMMC)?;
    l.w64(addr)?;
    l.w64(len)?;
    ack(l, "READ")?;
    l.w32(READ_CHUNK as u32)?;
    let mut buf = vec![0u8; READ_CHUNK];
    let mut done = 0u64;
    while done < len {
        let n = (len - done).min(READ_CHUNK as u64) as usize;
        l.read(&mut buf[..n])?;
        let sum = sum16(&buf[..n]);
        let dev = l.r16()?;
        if dev != sum {
            return Err(format!("checksum mismatch at byte {}: device {dev:#06x}, ours {sum:#06x}", addr + done));
        }
        l.w8(ACK)?;
        sink(&buf[..n])?;
        done += n as u64;
    }
    Ok(())
}

fn switch_to_user(l: &mut impl Link) -> Result<(), String> {
    l.w8(0x60)?; // SWITCH_PART
    ack(l, "SWITCH_PART")?;
    l.w8(PART_USER)?;
    ack(l, "SWITCH_PART user")
}

pub fn sum16(b: &[u8]) -> u16 {
    b.iter().fold(0u16, |s, &x| s.wrapping_add(x as u16))
}

/// Write `len` bytes from `src` to the eMMC user area at `addr`, in checksummed 1 MiB packets
/// (SDMMC_WRITE_DATA). The DA answers each packet with CONT_CHAR. The caller reads it back.
pub fn write_user(
    l: &mut impl Link,
    addr: u64,
    len: u64,
    src: &mut impl std::io::Read,
    mut progress: impl FnMut(u64),
) -> Result<(), String> {
    switch_to_user(l)?;
    l.w8(0x62)?;
    l.w8(STORAGE_SDMMC)?;
    l.w8(PART_USER)?;
    l.w64(addr)?;
    l.w64(len)?;
    l.w32(READ_CHUNK as u32)?;
    ack(l, "SDMMC_WRITE_DATA")?;
    let mut buf = vec![0u8; READ_CHUNK];
    let mut done = 0u64;
    while done < len {
        let n = (len - done).min(READ_CHUNK as u64) as usize;
        src.read_exact(&mut buf[..n]).map_err(|e| format!("backup file: {e}"))?;
        l.w8(ACK)?;
        l.write(&buf[..n])?;
        l.w16(sum16(&buf[..n]))?;
        match l.r8()? {
            CONT_CHAR => {}
            r => return Err(format!("Download Agent stopped the write at byte {}: {r:#04x}", addr + done)),
        }
        done += n as u64;
        progress(done);
    }
    Ok(())
}

/// Reboot the player normally (DA ENABLE_WATCHDOG, timeout 0).
pub fn reboot(l: &mut impl Link) -> Result<(), String> {
    l.w8(0xdb)?;
    l.w32(0)?;
    l.write(&[0, 0, 0, 1])?;
    ack(l, "ENABLE_WATCHDOG")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Replays scripted device bytes and records what the host sent.
    struct Fake {
        rx: std::collections::VecDeque<u8>,
        tx: Vec<u8>,
    }
    impl Link for Fake {
        fn read(&mut self, buf: &mut [u8]) -> Result<(), String> {
            for b in buf {
                *b = self.rx.pop_front().ok_or("fake ran dry")?;
            }
            Ok(())
        }
        fn write(&mut self, buf: &[u8]) -> Result<(), String> {
            self.tx.extend_from_slice(buf);
            Ok(())
        }
        fn flush(&mut self) {}
    }

    #[test]
    fn read_checks_each_chunk_and_acks_it() {
        let data: Vec<u8> = (0..READ_CHUNK + 100).map(|i| (i * 7) as u8).collect();
        let mut rx = vec![ACK, ACK, ACK]; // switch part, part, read
        for c in data.chunks(READ_CHUNK) {
            rx.extend_from_slice(c);
            rx.extend_from_slice(&c.iter().fold(0u16, |s, &b| s.wrapping_add(b as u16)).to_be_bytes());
        }
        let mut f = Fake { rx: rx.into(), tx: vec![] };
        let mut got = vec![];
        read_user(&mut f, 0x200, data.len() as u64, |c| Ok(got.extend_from_slice(c))).unwrap();
        assert_eq!(got, data);
        // READ command: op, host OS, storage, addr, len, then packet size; one ACK per chunk.
        assert_eq!(&f.tx[2..5], &[0xd6, 12, 2]);
        assert_eq!(&f.tx[5..13], &0x200u64.to_be_bytes());
        assert_eq!(&f.tx[f.tx.len() - 2..], &[ACK, ACK]);

        // One flipped byte fails the chunk instead of landing in the backup.
        let mut rx = vec![ACK, ACK, ACK];
        rx.extend_from_slice(&[1, 2, 3]);
        rx.extend_from_slice(&7u16.to_be_bytes());
        let mut f = Fake { rx: rx.into(), tx: vec![] };
        assert!(read_user(&mut f, 0, 3, |_| Ok(())).unwrap_err().contains("checksum"));
    }

    #[test]
    fn write_sends_ack_packet_checksum_and_stops_on_anything_but_cont() {
        let data: Vec<u8> = (0..READ_CHUNK + 5).map(|i| (i * 3) as u8).collect();
        let mut f = Fake { rx: vec![ACK, ACK, ACK, CONT_CHAR, CONT_CHAR].into(), tx: vec![] };
        write_user(&mut f, 0x2180000, data.len() as u64, &mut &data[..], |_| {}).unwrap();
        let t = &f.tx;
        // switch part, then op, storage, part, addr, len, packet size
        assert_eq!(&t[..5], &[0x60, PART_USER, 0x62, 2, PART_USER]);
        assert_eq!(&t[5..13], &0x2180000u64.to_be_bytes());
        assert_eq!(&t[13..21], &(data.len() as u64).to_be_bytes());
        assert_eq!(&t[21..25], &(READ_CHUNK as u32).to_be_bytes());
        // ACK, the packet, its checksum; twice
        assert_eq!(t[25], ACK);
        assert_eq!(&t[26..26 + READ_CHUNK], &data[..READ_CHUNK]);
        let c = 26 + READ_CHUNK;
        assert_eq!(&t[c..c + 2], &sum16(&data[..READ_CHUNK]).to_be_bytes());
        assert_eq!(t[c + 2], ACK);
        assert_eq!(&t[c + 3..c + 8], &data[READ_CHUNK..]);
        assert_eq!(t.len(), c + 10);

        let mut f = Fake { rx: vec![ACK, ACK, ACK, 0xa5].into(), tx: vec![] };
        assert!(write_user(&mut f, 0, 4, &mut &[1u8, 2, 3, 4][..], |_| {}).unwrap_err().contains("stopped the write"));
    }

    #[test]
    fn da_file_header_and_stage_pick() {
        let mut b = vec![0u8; DA_HEADER + DA_ENTRY];
        b[32..38].copy_from_slice(b"MT8590");
        b[96..100].copy_from_slice(&4u32.to_le_bytes());
        b[100..104].copy_from_slice(&0x2266_8899u32.to_le_bytes());
        b[104..108].copy_from_slice(&1u32.to_le_bytes());
        let e = DA_HEADER;
        b[e..e + 2].copy_from_slice(&0xdadau16.to_le_bytes());
        b[e + 2..e + 4].copy_from_slice(&0x8590u16.to_le_bytes());
        b[e + 18..e + 20].copy_from_slice(&3u16.to_le_bytes());
        // region 0 unsigned (skipped), 1 = stage 1, 2 = stage 2
        for (r, (len, sig)) in [(0x100u32, 0u32), (0x200, 0x100), (0x300, 0x100)].iter().enumerate() {
            let o = e + 20 + r * 20;
            b[o + 4..o + 8].copy_from_slice(&len.to_le_bytes());
            b[o + 8..o + 12].copy_from_slice(&(0x20_0000 + r as u32).to_le_bytes());
            b[o + 12..o + 16].copy_from_slice(&(len - sig).to_le_bytes());
            b[o + 16..o + 20].copy_from_slice(&sig.to_le_bytes());
        }
        let (desc, entries) = parse_da(&b).unwrap();
        assert_eq!(desc, "MT8590");
        let (s1, s2) = pick_stages(&entries, 0x8590, 0xca00, 0).unwrap();
        assert_eq!((s1.start_addr, s2.start_addr), (0x20_0001, 0x20_0002));
        assert!(pick_stages(&entries, 0x6580, 0, 0).is_err());
        assert_eq!(xor16(&[0x01, 0x02, 0x03]), 0x0201 ^ 0x0003);
    }
}
