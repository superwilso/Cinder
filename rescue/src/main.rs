//! cinder-rescue — back up a Sony NW-A50-series Walkman's eMMC over MediaTek's download mode.
//!
//! Download mode lives in the preloader, before any kernel, so it still answers on a player that
//! boot-loops. `info` proves the link, `dump` copies a range of the eMMC user area to a new file with
//! a manifest, `verify` re-checks a backup, and `restore` writes chosen partitions back from this
//! player's own backup (the only command that writes; see `restore` for its guards).
//!
//! The Download Agent (DA) is MediaTek's and is not shipped here; pass the one you already have
//! (wbrt carries `DA.bin`; the flash tool's README names `MTK_AllInOne_DA_5.2136.bin`).
//!
//! Windows needs a WinUSB driver on VID_0E8D&PID_2000 ("MT65xx Preloader"), the one wbrt installs.

mod backup;
mod mtk;
mod sha256;

use mtk::Link;
use std::io::{Read, Seek, SeekFrom, Write};
use std::time::{Duration, Instant};

/// What wbrt backs up: the user area from 0 to here (mbr through the first ~400 MB of contents).
const WBRT_LEN: u64 = 2_885_156_864;

struct Usb {
    h: rusb::DeviceHandle<rusb::GlobalContext>,
    buf: [u8; 512],
    pos: usize,
    avail: usize,
    timeout: Duration,
}

impl Link for Usb {
    fn read(&mut self, out: &mut [u8]) -> Result<(), String> {
        let mut off = 0;
        while off < out.len() {
            if self.avail == 0 {
                // One 512-byte packet per transfer, as the flash tool does. 1 MiB transfers stalled
                // the pipe 120 MiB into the first dump (2026-10-26).
                self.avail = self.h.read_bulk(0x81, &mut self.buf, self.timeout).map_err(|e| format!("USB read: {e}"))?;
                self.pos = 0;
            }
            let n = (out.len() - off).min(self.avail);
            out[off..off + n].copy_from_slice(&self.buf[self.pos..self.pos + n]);
            off += n;
            self.pos += n;
            self.avail -= n;
        }
        Ok(())
    }

    fn write(&mut self, data: &[u8]) -> Result<(), String> {
        let mut off = 0;
        while off < data.len() {
            off += self.h.write_bulk(0x01, &data[off..], self.timeout).map_err(|e| format!("USB write: {e}"))?;
        }
        Ok(())
    }

    fn flush(&mut self) {
        self.avail = 0;
    }
}

fn wait_for_player() -> Result<Usb, String> {
    println!("Waiting for the player in download mode (5 minutes):");
    println!("  1. Unplug the cable and turn the player off (hold power about 10 s if it is looping)");
    println!("  2. Hold Play and Volume Down");
    println!("  3. Plug the cable in, and let go when this window moves on");
    let start = Instant::now();
    let h = loop {
        if let Some(h) = rusb::open_device_with_vid_pid(0x0e8d, 0x2000) {
            break h;
        }
        if start.elapsed() > Duration::from_secs(300) {
            return Err("no player in download mode (USB 0e8d:2000) appeared".into());
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let _ = h.set_auto_detach_kernel_driver(true);
    let ifaces = h.device().active_config_descriptor().map(|c| c.num_interfaces()).unwrap_or(1);
    for i in 0..ifaces {
        let _ = h.claim_interface(i);
    }
    // CDC SET_LINE_CODING with no data, as the flash tool sends before the handshake.
    h.write_control(0x21, 0x20, 0, 0, &[], Duration::from_secs(1)).map_err(|e| format!("USB setup: {e}"))?;
    Ok(Usb { h, buf: [0; 512], pos: 0, avail: 0, timeout: Duration::from_secs(1) })
}

/// Handshake, load the DA, and leave stage 2 running. Prints what it learns and returns it as
/// manifest header lines.
fn connect(da: &[u8]) -> Result<(Usb, Vec<String>), String> {
    let (desc, entries) = mtk::parse_da(da)?;
    let mut usb = wait_for_player()?;
    mtk::handshake(&mut usb)?;
    let chip = mtk::chip(&mut usb)?;
    println!("Chip {:#06x}, hardware {:#06x}, software {:#06x}", chip.hw_code, chip.hw_ver, chip.sw_ver);
    let (s1, s2) = mtk::pick_stages(&entries, chip.hw_code, chip.hw_ver, chip.sw_ver)?;
    println!("Loading the Download Agent ({desc})");
    let cid = mtk::load_da(&mut usb, da, s1, s2)?;
    println!("eMMC CID {:08X} {:08X} {:08X} {:08X}", cid[0], cid[1], cid[2], cid[3]);
    usb.timeout = Duration::from_secs(10);
    let header = vec![
        format!("chip {:#06x} {:#06x} {:#06x}", chip.hw_code, chip.hw_ver, chip.sw_ver),
        format!("emmc-cid {:08X} {:08X} {:08X} {:08X}", cid[0], cid[1], cid[2], cid[3]),
    ];
    Ok((usb, header))
}

fn dump(da: &[u8], out: &str, addr: u64, len: u64) -> Result<(), String> {
    // A new file only: never overwrite a backup that already exists. Checked before the wait,
    // created after it, so a run that never connects leaves nothing behind.
    let manifest = format!("{out}.manifest");
    for p in [out, &manifest] {
        if std::path::Path::new(p).exists() {
            return Err(format!("{p} already exists; pick a new name"));
        }
    }
    let (mut usb, header) = connect(da)?;
    let mut hashes = backup::Hashes::new(addr, len);
    let file = std::fs::OpenOptions::new().write(true).create_new(true).open(out).map_err(|e| format!("{out}: {e}"))?;
    let mut file = std::io::BufWriter::with_capacity(1 << 20, file);
    println!("Reading {len} bytes from {addr:#x} into {out}");
    let start = Instant::now();
    let mut done = 0u64;
    let mut shown = 0u64;
    let res = mtk::read_user(&mut usb, addr, len, |chunk| {
        file.write_all(chunk).map_err(|e| format!("{out}: {e}"))?;
        hashes.feed(addr + done, chunk);
        done += chunk.len() as u64;
        if done - shown >= 64 << 20 || done == len {
            shown = done;
            let s = start.elapsed().as_secs_f64().max(0.001);
            print!("\r  {} / {} MiB, {:.1} MiB/s   ", done >> 20, len >> 20, done as f64 / s / 1048576.0);
            let _ = std::io::stdout().flush();
        }
        Ok(())
    });
    file.flush().map_err(|e| format!("{out}: {e}"))?;
    if let Err(e) = res {
        // Every byte in the file passed its checksum, so the rest can go to a second file.
        return Err(format!(
            "{e}\n{out} holds the first {done} bytes, all checked. Restart the player (unplug, hold power 10 s), \
             then fetch the rest into a new file with: --addr {:#x} --len {:#x}",
            addr + done,
            len - done
        ));
    }
    drop(file);
    println!("\nDone in {:.0} s. Restarting the player.", start.elapsed().as_secs_f64());
    mtk::reboot(&mut usb)?;
    // Hashes of the bytes as received; verify then proves the file on disk holds the same.
    backup::write_manifest(&manifest, &header, addr, len, &hashes.finish())?;
    verify(out)
}

/// Re-read a backup and check it against its manifest, or write a manifest for one that has
/// none (a wbrt backup, say). Checks the partition layout when the backup starts at 0.
fn verify(path: &str) -> Result<(), String> {
    let manifest = format!("{path}.manifest");
    let m = std::path::Path::new(&manifest).exists().then(|| backup::read_manifest(&manifest)).transpose()?;
    let (addr, len) = match &m {
        Some(m) => (m.addr, m.len),
        None => (0, std::fs::metadata(path).map_err(|e| format!("{path}: {e}"))?.len()),
    };
    if addr == 0 && len >= 0x500000 {
        let mut head = vec![0u8; 0x500000];
        std::io::Read::read_exact(&mut std::fs::File::open(path).map_err(|e| format!("{path}: {e}"))?, &mut head)
            .map_err(|e| format!("{path}: {e}"))?;
        backup::check_layout(&head)?;
        println!("Partition table: an NW-A50-series layout");
    }
    println!("Reading {path} back ({} MiB)", len >> 20);
    let sums = backup::hash_file(path, addr, len, |pos| {
        if pos % (256 << 20) == 0 || pos == len {
            print!("\r  {} / {} MiB   ", pos >> 20, len >> 20);
            let _ = std::io::stdout().flush();
        }
    })?;
    println!();
    let Some(m) = m else {
        backup::write_manifest(&manifest, &["source: no manifest before; hashed from the file as it is".into()], addr, len, &sums)?;
        println!("No manifest: wrote {manifest} from the file as it is now ({} partitions).", sums.len());
        return Ok(());
    };
    let mut bad = vec![];
    for (r, h) in &sums {
        match m.sums.iter().find(|s| s.0 == r.name) {
            Some(s) if s.1 == r.start && s.2 == r.end - r.start && &s.3 == h => {}
            _ => bad.push(r.name),
        }
    }
    if !bad.is_empty() || m.sums.len() != sums.len() {
        return Err(format!("{path} does not match its manifest: {}", if bad.is_empty() { "partition list differs".into() } else { bad.join(", ") }));
    }
    println!("Verified: all {} partitions match {manifest}.", sums.len());
    Ok(())
}

/// Partitions a boot loop can break, and the only ones restore will write. Everything else (the
/// partition table, bootloader, nvram, nvp, pro_info, seccfg, tee, contents) is left as it is.
const RESTORABLE: &[&str] = &["bootimg", "recovery", "android", "usrdata", "cache", "var", "db"];

fn sha_of(src: &mut impl Read, len: u64) -> Result<String, String> {
    let mut h = sha256::Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    let mut left = len;
    while left > 0 {
        let n = left.min(buf.len() as u64) as usize;
        src.read_exact(&mut buf[..n]).map_err(|e| format!("backup file: {e}"))?;
        h.update(&buf[..n]);
        left -= n as u64;
    }
    Ok(h.hex())
}

fn device_sha(usb: &mut Usb, start: u64, len: u64) -> Result<String, String> {
    let mut h = sha256::Sha256::new();
    mtk::read_user(usb, start, len, |c| Ok(h.update(c)))?;
    Ok(h.hex())
}

/// Write partitions back from a backup. In order, it refuses unless:
///   1. every chosen partition is in RESTORABLE and wholly inside the backup;
///   2. the backup's partition table is an A50-series one and the chosen partitions still hash to
///      what the manifest recorded when they were read off the player;
///   3. the player's chip and eMMC CID equal the manifest's (a backup only goes back to its player);
///   4. the player's own partition table passes the same layout check.
/// Then per partition: read and hash what the player has now; leave it alone if it already
/// matches; otherwise write it and read it back, which must hash to the manifest's value.
fn restore(da: &[u8], from: &str, parts: &str, dry_run: bool, even_if_same: bool) -> Result<(), String> {
    let manifest = format!("{from}.manifest");
    let m = backup::read_manifest(&manifest)?;
    let mut chosen = vec![];
    for name in parts.split(',') {
        if !RESTORABLE.contains(&name) {
            return Err(format!("{name} is not restorable; restore writes only {}", RESTORABLE.join(", ")));
        }
        let &(_, start, size) = backup::TABLE.iter().find(|t| t.0 == name).expect("RESTORABLE names are in TABLE");
        let sum = m
            .sums
            .iter()
            .find(|s| s.0 == name && s.1 == start && s.2 == size)
            .ok_or_else(|| format!("{from} does not hold all of {name}"))?;
        chosen.push((name, start, size, sum.3.clone()));
    }
    if m.addr != 0 {
        return Err("restore needs a backup that starts at 0, so its partition table can be checked".into());
    }
    let mut f = std::fs::File::open(from).map_err(|e| format!("{from}: {e}"))?;
    let mut head = vec![0u8; 0x500000];
    f.read_exact(&mut head).map_err(|e| format!("{from}: {e}"))?;
    backup::check_layout(&head)?;
    for (name, start, size, want) in &chosen {
        f.seek(SeekFrom::Start(*start)).map_err(|e| format!("{from}: {e}"))?;
        if &sha_of(&mut f, *size)? != want {
            return Err(format!("{name} in {from} no longer matches its manifest; not using it"));
        }
        println!("Backup's {name} checks out ({} MiB)", size >> 20);
    }
    let field = |k: &str| m.header.iter().find(|h| h.starts_with(k)).cloned();
    let (Some(chip), Some(cid)) = (field("chip "), field("emmc-cid ")) else {
        return Err(format!("{manifest} has no chip and eMMC CID, so nothing proves it came from this player"));
    };

    let (mut usb, header) = connect(da)?;
    let stuck = "
The player is still in download mode: unplug it and hold power about 10 s.";
    if header[0] != chip || header[1] != cid {
        return Err(format!("this player is not the one {from} came from:
  player {} / {}
  backup {chip} / {cid}{stuck}", header[0], header[1]));
    }
    let mut dev_head = Vec::with_capacity(0x500000);
    mtk::read_user(&mut usb, 0, 0x500000, |c| Ok(dev_head.extend_from_slice(c)))?;
    backup::check_layout(&dev_head).map_err(|e| format!("the player's own partition table: {e}{stuck}"))?;
    println!("Same player as the backup, same partition layout");

    for (name, start, size, want) in &chosen {
        let now = device_sha(&mut usb, *start, *size).map_err(|e| format!("{e}{stuck}"))?;
        if &now == want && !even_if_same {
            println!("{name}: already the same as the backup, left alone");
            continue;
        }
        let what = if &now == want { "the same as the backup (rewriting it anyway)" } else { "different from the backup" };
        if dry_run {
            println!("{name}: {what}; would write {} MiB", size >> 20);
            continue;
        }
        println!("{name}: {what}; writing {} MiB", size >> 20);
        f.seek(SeekFrom::Start(*start)).map_err(|e| format!("{from}: {e}"))?;
        let t = Instant::now();
        mtk::write_user(&mut usb, *start, *size, &mut (&mut f).take(*size), |done| {
            if done % (16 << 20) == 0 || done == *size {
                print!("\r  {} / {} MiB   ", done >> 20, size >> 20);
                let _ = std::io::stdout().flush();
            }
        })
        .map_err(|e| format!("{e}{stuck}"))?;
        println!("in {:.0} s; reading it back", t.elapsed().as_secs_f64());
        let after = device_sha(&mut usb, *start, *size).map_err(|e| format!("{e}{stuck}"))?;
        if &after != want {
            return Err(format!("{name} read back differently from the backup. Run restore again, or restore with wbrt.{stuck}"));
        }
        println!("{name}: written and read back, matches the backup");
    }
    println!("Restarting the player.");
    mtk::reboot(&mut usb)
}

fn num(s: &str) -> Result<u64, String> {
    match s.strip_prefix("0x") {
        Some(h) => u64::from_str_radix(h, 16),
        None => s.parse(),
    }
    .map_err(|_| format!("not a number: {s}"))
}

fn help() {
    println!("cinder-rescue {} — read a Walkman's eMMC over MediaTek download mode", env!("CARGO_PKG_VERSION"));
    println!();
    println!("  cinder-rescue info --da DA.bin                  connect, show the chip and eMMC, restart");
    println!("  cinder-rescue dump --da DA.bin --out FILE       copy what wbrt copies ({WBRT_LEN} bytes from 0)");
    println!("                [--addr N] [--len N]            ...or another range of the user area");
    println!("  cinder-rescue verify FILE                       re-read a backup against FILE.manifest,");
    println!("                                                  or write one (for a wbrt backup)");
    println!();
    println!("  cinder-rescue restore --da DA.bin --from FILE --parts bootimg[,android,...]");
    println!("                [--dry-run]                     check everything, write nothing");
    println!("                [--even-if-same]                rewrite partitions that already match");
    println!("      writes only {} from this player's own backup;", RESTORABLE.join(", "));
    println!("      anything that already matches is left alone, and every write is read back");
    println!();
    println!("  cinder-rescue licences                          the notices this program carries");
    println!();
    println!("A dump writes FILE.manifest (chip, eMMC CID, SHA-256 per partition) and verifies itself.");
    println!("Only restore writes to the player.");
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let opt = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(String::as_str);
    let cmd = args.first().map(String::as_str).unwrap_or("");
    if cmd == "licences" {
        // The protocol facts are ported from an MIT tool, whose notice must travel with every copy;
        // libusb is linked statically under the LGPL 2.1 (its licence ships with each release).
        println!("{}", include_str!("../LICENSE-mediatek_flash_tool"));
        println!("libusb 1.0 is linked statically, under the GNU LGPL 2.1: https://libusb.info");
        println!("Source and relinking: build rescue/ from the tagged release with your own libusb.");
        return Ok(());
    }
    if cmd == "verify" {
        return verify(args.get(1).ok_or("verify needs a FILE")?);
    }
    if !matches!(cmd, "info" | "dump" | "restore") {
        help();
        return Ok(());
    }
    let da_path = opt("--da").ok_or("--da DA.bin is required")?;
    let da = std::fs::read(da_path).map_err(|e| format!("{da_path}: {e}"))?;
    if cmd == "info" {
        let (mut usb, _) = connect(&da)?;
        println!("Download mode works. Restarting the player.");
        return mtk::reboot(&mut usb);
    }
    if cmd == "restore" {
        let from = opt("--from").ok_or("--from FILE is required")?;
        let parts = opt("--parts").ok_or("--parts is required, e.g. --parts bootimg")?;
        let flag = |f: &str| args.iter().any(|a| a == f);
        return restore(&da, from, parts, flag("--dry-run"), flag("--even-if-same"));
    }
    let out = opt("--out").ok_or("--out FILE is required")?;
    let addr = opt("--addr").map(num).transpose()?.unwrap_or(0);
    let len = opt("--len").map(num).transpose()?.unwrap_or(WBRT_LEN);
    dump(&da, out, addr, len)
}

fn main() {
    if let Err(e) = run() {
        eprintln!("cinder-rescue: {e}");
        std::process::exit(1);
    }
}
