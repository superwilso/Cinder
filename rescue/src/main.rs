//! cinder-rescue — back up a Sony NW-A50-series Walkman's eMMC over MediaTek's download mode.
//!
//! Download mode lives in the preloader, before any kernel, so it still answers on a player that
//! boot-loops. `info` proves the link, `dump` copies a range of the eMMC user area to a new file with
//! a manifest, `verify` re-checks a backup, and `restore` writes chosen partitions back from this
//! player's own backup (the only command that writes; see `restore` for its guards).
//!
//! The Download Agent (DA) is MediaTek's and is not shipped here: `fetch` takes the verified one out
//! of wbrt's release, or `--da` names your own.
//!
//! Windows needs a WinUSB driver on VID_0E8D&PID_2000 ("MT65xx Preloader"): `driver` installs it
//! as wbrt does, and wbrt's own works too.

/// Everything cinder-rescue says goes to the terminal and to `LOG`, which the page shows.
static LOG: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

macro_rules! say {
    ($($t:tt)*) => {{
        let s = format!($($t)*);
        print!("{s}");
        let _ = std::io::Write::flush(&mut std::io::stdout());
        crate::LOG.lock().unwrap().push_str(&s);
    }};
}
macro_rules! sayln {
    () => { say!("\n") };
    ($($t:tt)*) => { say!("{}\n", format!($($t)*)) };
}

mod backup;
mod fetch;
mod mtk;
mod sha256;
mod web;

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
                self.avail = self
                    .h
                    .read_bulk(0x81, &mut self.buf, self.timeout)
                    .map_err(|e| format!("USB read: {e}"))?;
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
            off += self
                .h
                .write_bulk(0x01, &data[off..], self.timeout)
                .map_err(|e| format!("USB write: {e}"))?;
        }
        Ok(())
    }

    fn flush(&mut self) {
        self.avail = 0;
    }
}

fn wait_for_player() -> Result<Usb, String> {
    // rusb's global context panics when libusb cannot start (no USB at all, as in WSL).
    rusb::Context::new().map_err(|e| format!("no USB access here (libusb: {e})"))?;
    sayln!("Waiting for the player in download mode (5 minutes):");
    sayln!(
        "  1. Unplug the cable and turn the player off (hold power about 10 s if it is looping)"
    );
    sayln!("  2. Hold Play and Volume Down");
    sayln!("  3. Plug the cable in, and let go when this window moves on");
    let start = Instant::now();
    let mut refused: Option<Instant> = None;
    let h = loop {
        let found = rusb::devices().ok().and_then(|l| {
            l.iter().find(|d| {
                d.device_descriptor()
                    .is_ok_and(|x| x.vendor_id() == 0x0e8d && x.product_id() == 0x2000)
            })
        });
        match found.map(|d| d.open()) {
            Some(Ok(h)) => break h,
            // Seen but not ours to open for a second and a half: no driver, or no permission.
            Some(Err(e))
                if refused.get_or_insert_with(Instant::now).elapsed()
                    > Duration::from_millis(1500) =>
            {
                let fix = if cfg!(windows) {
                    "Windows has no WinUSB driver for it: run `cinder-rescue driver` from a terminal opened as administrator (or install wbrt)"
                } else {
                    "run with sudo, or add a udev rule for 0e8d:2000"
                };
                return Err(format!("the player is in download mode but cannot be opened ({e}).\n{fix}, then try again"));
            }
            Some(Err(_)) => {}
            None => refused = None,
        }
        if web::CANCEL.swap(false, std::sync::atomic::Ordering::Relaxed) {
            return Err("stopped before the player was found; nothing was read or written".into());
        }
        if start.elapsed() > Duration::from_secs(300) {
            return Err("no player in download mode (USB 0e8d:2000) appeared".into());
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let _ = h.set_auto_detach_kernel_driver(true);
    let ifaces = h
        .device()
        .active_config_descriptor()
        .map(|c| c.num_interfaces())
        .unwrap_or(1);
    for i in 0..ifaces {
        let _ = h.claim_interface(i);
    }
    // CDC SET_LINE_CODING with no data, as the flash tool sends before the handshake.
    h.write_control(0x21, 0x20, 0, 0, &[], Duration::from_secs(1))
        .map_err(|e| format!("USB setup: {e}"))?;
    Ok(Usb {
        h,
        buf: [0; 512],
        pos: 0,
        avail: 0,
        timeout: Duration::from_secs(1),
    })
}

/// Handshake, load the DA, and leave stage 2 running. Prints what it learns and returns it as
/// manifest header lines.
fn connect(da: &[u8]) -> Result<(Usb, Vec<String>), String> {
    let (desc, entries) = mtk::parse_da(da)?;
    let mut usb = wait_for_player()?;
    mtk::handshake(&mut usb)?;
    let chip = mtk::chip(&mut usb)?;
    sayln!(
        "Chip {:#06x}, hardware {:#06x}, software {:#06x}",
        chip.hw_code,
        chip.hw_ver,
        chip.sw_ver
    );
    let (s1, s2) = mtk::pick_stages(&entries, chip.hw_code, chip.hw_ver, chip.sw_ver)?;
    sayln!("Loading the Download Agent ({desc})");
    let cid = mtk::load_da(&mut usb, da, s1, s2)?;
    sayln!(
        "eMMC CID {:08X} {:08X} {:08X} {:08X}",
        cid[0],
        cid[1],
        cid[2],
        cid[3]
    );
    usb.timeout = Duration::from_secs(10);
    let header = vec![
        format!(
            "chip {:#06x} {:#06x} {:#06x}",
            chip.hw_code, chip.hw_ver, chip.sw_ver
        ),
        format!(
            "emmc-cid {:08X} {:08X} {:08X} {:08X}",
            cid[0], cid[1], cid[2], cid[3]
        ),
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
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(out)
        .map_err(|e| format!("{out}: {e}"))?;
    let mut file = std::io::BufWriter::with_capacity(1 << 20, file);
    sayln!("Reading {len} bytes from {addr:#x} into {out}");
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
            say!(
                "\r  {} / {} MiB, {:.1} MiB/s   ",
                done >> 20,
                len >> 20,
                done as f64 / s / 1048576.0
            );
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
    sayln!(
        "\nDone in {:.0} s. Restarting the player.",
        start.elapsed().as_secs_f64()
    );
    mtk::reboot(&mut usb)?;
    // Hashes of the bytes as received; verify then proves the file on disk holds the same.
    backup::write_manifest(&manifest, &header, addr, len, &hashes.finish())?;
    verify(out)
}

/// Re-read a backup and check it against its manifest, or write a manifest for one that has
/// none (a wbrt backup, say). Checks the partition layout when the backup starts at 0.
fn verify(path: &str) -> Result<(), String> {
    let manifest = format!("{path}.manifest");
    let m = std::path::Path::new(&manifest)
        .exists()
        .then(|| backup::read_manifest(&manifest))
        .transpose()?;
    let (addr, len) = match &m {
        Some(m) => (m.addr, m.len),
        None => (
            0,
            std::fs::metadata(path)
                .map_err(|e| format!("{path}: {e}"))?
                .len(),
        ),
    };
    if addr == 0 && len >= 0x500000 {
        let mut head = vec![0u8; 0x500000];
        std::io::Read::read_exact(
            &mut std::fs::File::open(path).map_err(|e| format!("{path}: {e}"))?,
            &mut head,
        )
        .map_err(|e| format!("{path}: {e}"))?;
        backup::check_layout(&head)?;
        sayln!("Partition table: an NW-A50-series layout");
    }
    sayln!("Reading {path} back ({} MiB)", len >> 20);
    let sums = backup::hash_file(path, addr, len, |pos| {
        if pos % (256 << 20) == 0 || pos == len {
            say!("\r  {} / {} MiB   ", pos >> 20, len >> 20);
            let _ = std::io::stdout().flush();
        }
    })?;
    sayln!();
    let Some(m) = m else {
        backup::write_manifest(
            &manifest,
            &["source: no manifest before; hashed from the file as it is".into()],
            addr,
            len,
            &sums,
        )?;
        sayln!(
            "No manifest: wrote {manifest} from the file as it is now ({} partitions).",
            sums.len()
        );
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
        return Err(format!(
            "{path} does not match its manifest: {}",
            if bad.is_empty() {
                "partition list differs".into()
            } else {
                bad.join(", ")
            }
        ));
    }
    sayln!("Verified: all {} partitions match {manifest}.", sums.len());
    Ok(())
}

/// Restore writes any partition in the table but `contents`, your music: a backup holds only its
/// first ~400 MB, and writing that over a drive whose files have moved on since would corrupt it
/// (a wbrt restore does exactly that).
fn restorable(name: &str) -> bool {
    name != "contents" && backup::TABLE.iter().any(|t| t.0 == name)
}

fn sha_of(src: &mut impl Read, len: u64) -> Result<String, String> {
    let mut h = sha256::Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    let mut left = len;
    while left > 0 {
        let n = left.min(buf.len() as u64) as usize;
        src.read_exact(&mut buf[..n])
            .map_err(|e| format!("backup file: {e}"))?;
        h.update(&buf[..n]);
        left -= n as u64;
    }
    Ok(h.hex())
}

/// Pieces compared, written and read back.
const PIECE: usize = 1 << 20;
/// Read from the player per pass.
const WINDOW: u64 = 64 << 20;

/// Make `size` bytes at `start` on the player equal `src` (the backup at the same place), writing
/// only the 1 MiB pieces that differ, or every piece with `all`, and reading each written run back.
/// Returns the bytes that differed (and, unless `dry_run`, were written).
fn sync(
    l: &mut impl mtk::Link,
    src: &mut impl Read,
    start: u64,
    size: u64,
    all: bool,
    dry_run: bool,
    mut progress: impl FnMut(u64),
) -> Result<u64, String> {
    let mut changed = 0;
    let mut pos = 0;
    while pos < size {
        let n = (size - pos).min(WINDOW) as usize;
        let mut want = vec![0u8; n];
        src.read_exact(&mut want)
            .map_err(|e| format!("backup file: {e}"))?;
        let mut have = Vec::with_capacity(n);
        mtk::read_user(l, start + pos, n as u64, |c| Ok(have.extend_from_slice(c)))?;
        let differs = |i: usize| {
            let j = (i + PIECE).min(n);
            all || have[i..j] != want[i..j]
        };
        let mut i = 0;
        while i < n {
            if !differs(i) {
                i += PIECE;
                continue;
            }
            let mut j = i;
            while j < n && differs(j) {
                j = (j + PIECE).min(n);
            }
            let at = start + pos + i as u64;
            if !dry_run {
                mtk::write_user(l, at, (j - i) as u64, &mut &want[i..j], |_| {})?;
                let mut back = Vec::with_capacity(j - i);
                mtk::read_user(l, at, (j - i) as u64, |c| Ok(back.extend_from_slice(c)))?;
                if back != want[i..j] {
                    return Err(format!(
                        "{at:#x} read back differently from the backup. Run restore again, or restore with wbrt."
                    ));
                }
            }
            changed += (j - i) as u64;
            i = j;
        }
        pos += n as u64;
        progress(pos);
    }
    Ok(changed)
}

/// Write partitions back from a backup of this player. In order, it refuses unless:
///   1. every chosen partition is restorable and wholly inside the backup;
///   2. the backup's partition table is an A50-series one and the chosen partitions still hash to
///      what the manifest recorded;
///   3. the player is the one the backup came from: the same chip and eMMC ID, or for a backup with
///      no ID (wbrt's), the same serial number in `nvp`;
///   4. the player's own partition table passes the same layout check, unless the table itself
///      (`mbr`) is among what is being put back.
/// Then per partition: compare the player with the backup 64 MiB at a time, write only the 1 MiB
/// pieces that differ, and read each written piece back; it must equal the backup.
fn restore(
    da: &[u8],
    from: &str,
    parts: &str,
    dry_run: bool,
    even_if_same: bool,
) -> Result<(), String> {
    let manifest = format!("{from}.manifest");
    let m = backup::read_manifest(&manifest).map_err(|e| {
        format!("{e}\nA wbrt backup has no manifest yet: run `cinder-rescue verify {from}` once")
    })?;
    if m.addr != 0 {
        return Err(
            "restore needs a backup that starts at 0, so its partition table can be checked".into(),
        );
    }
    let in_backup = |name: &str, start: u64, size: u64| {
        m.sums
            .iter()
            .find(|s| s.0 == name && s.1 == start && s.2 == size)
    };
    let names: Vec<&str> = if parts == "all" {
        backup::TABLE
            .iter()
            .filter(|t| restorable(t.0) && in_backup(t.0, t.1, t.2).is_some())
            .map(|t| t.0)
            .collect()
    } else {
        parts.split(',').collect()
    };
    let mut chosen = vec![];
    for name in names {
        if !restorable(name) {
            return Err(format!(
                "{name} is not restorable: restore writes any partition but contents (your music), or `all`"
            ));
        }
        let &(_, start, size) = backup::TABLE.iter().find(|t| t.0 == name).unwrap();
        let sum = in_backup(name, start, size)
            .ok_or_else(|| format!("{from} does not hold all of {name}"))?;
        chosen.push((name, start, size, sum.3.clone()));
    }
    let mut f = std::fs::File::open(from).map_err(|e| format!("{from}: {e}"))?;
    let mut head = vec![0u8; 0x500000];
    f.read_exact(&mut head)
        .map_err(|e| format!("{from}: {e}"))?;
    backup::check_layout(&head)?;
    for (name, start, size, want) in &chosen {
        f.seek(SeekFrom::Start(*start))
            .map_err(|e| format!("{from}: {e}"))?;
        if &sha_of(&mut f, *size)? != want {
            return Err(format!(
                "{name} in {from} no longer matches its manifest; not using it"
            ));
        }
    }
    let total: u64 = chosen.iter().map(|c| c.2).sum();
    sayln!(
        "The backup checks out: {} partitions, {} MiB",
        chosen.len(),
        total >> 20
    );
    let field = |k: &str| m.header.iter().find(|h| h.starts_with(k)).cloned();
    let id = field("chip ").zip(field("emmc-cid "));
    let serial = match id {
        Some(_) => None,
        None => {
            let mut b = [0u8; 16];
            f.seek(SeekFrom::Start(backup::SERIAL_AT))
                .and_then(|_| f.read_exact(&mut b))
                .map_err(|e| format!("{from}: {e}"))?;
            let s = backup::serial(&b).ok_or_else(|| {
                format!("{from} records no eMMC ID and holds no serial number, so nothing proves it came from this player")
            })?;
            sayln!("No eMMC ID in this backup (a wbrt backup): the player must have serial {s}");
            Some(s)
        }
    };

    let (mut usb, header) = connect(da)?;
    let stuck = "\nThe player is still in download mode: unplug it and hold power about 10 s.";
    if let Some((chip, cid)) = &id {
        if header[0] != *chip || header[1] != *cid {
            return Err(format!(
                "this player is not the one {from} came from:\n  player {} / {}\n  backup {chip} / {cid}{stuck}",
                header[0], header[1]
            ));
        }
        sayln!("Same player as the backup (chip and eMMC ID)");
    }
    if let Some(want) = &serial {
        let mut b = vec![];
        mtk::read_user(&mut usb, backup::SERIAL_AT, 512, |c| {
            Ok(b.extend_from_slice(c))
        })?;
        match backup::serial(&b) {
            Some(s) if s == *want => sayln!("Same player as the backup (serial {s})"),
            s => {
                return Err(format!(
                    "this player is not the one {from} came from: serial {} here, {want} in the backup{stuck}",
                    s.as_deref().unwrap_or("(none readable)")
                ))
            }
        }
    }
    if chosen.iter().any(|c| c.0 == "mbr") {
        sayln!("Putting the partition table back too, so the player's own is not checked");
    } else {
        let mut dev_head = Vec::with_capacity(0x500000);
        mtk::read_user(&mut usb, 0, 0x500000, |c| Ok(dev_head.extend_from_slice(c)))?;
        backup::check_layout(&dev_head)
            .map_err(|e| format!("the player's own partition table: {e}{stuck}"))?;
        sayln!("The player's partition table is an A50-series one");
    }

    let mut boot_chain = false;
    for (name, start, size, _) in &chosen {
        f.seek(SeekFrom::Start(*start))
            .map_err(|e| format!("{from}: {e}"))?;
        let t = Instant::now();
        let changed = sync(
            &mut usb,
            &mut (&mut f).take(*size),
            *start,
            *size,
            even_if_same,
            dry_run,
            |done| {
                say!("\r  {name}: {} / {} MiB   ", done >> 20, size >> 20);
                let _ = std::io::stdout().flush();
            },
        )
        .map_err(|e| format!("\n{name}: {e}{stuck}"))?;
        let mib = (changed + (1 << 20) - 1) >> 20;
        say!("\r");
        if changed == 0 {
            sayln!("{name}: already the same as the backup, left alone");
        } else if dry_run {
            sayln!(
                "{name}: {mib} of {} MiB differ; would write them",
                size >> 20
            );
        } else {
            boot_chain |= *start < 0x2180000; // before bootimg: the table, nvram, LK…
            sayln!(
                "{name}: wrote the {mib} of {} MiB that differed and read them back ({:.0} s)",
                size >> 20,
                t.elapsed().as_secs_f64()
            );
        }
    }
    if boot_chain {
        // wbrt's own installer notes that the watchdog restart does not boot after such a write.
        sayln!("Done. Unplug the cable, then hold power about 10 s to start the player.");
        return Ok(());
    }
    sayln!("Done. Restarting the player.");
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
    sayln!(
        "cinder-rescue {} — back up and restore a Walkman's eMMC over MediaTek download mode",
        env!("CARGO_PKG_VERSION")
    );
    sayln!();
    sayln!("  cinder-rescue                                   open the page (the same commands, in your browser)");
    sayln!("  cinder-rescue info                              connect, show the chip and eMMC, restart");
    sayln!("  cinder-rescue dump --out FILE                   copy what wbrt copies ({WBRT_LEN} bytes from 0)");
    sayln!("                [--addr N] [--len N]            ...or another range of the user area");
    sayln!(
        "  cinder-rescue verify FILE                       re-read a backup against FILE.manifest,"
    );
    sayln!("                                                  or write one (for a wbrt backup)");
    sayln!();
    sayln!("  cinder-rescue restore --from FILE --parts bootimg[,android,...]   or --parts all");
    sayln!(
        "                [--dry-run]                     compare with the player, write nothing"
    );
    sayln!("                [--even-if-same]                rewrite pieces that already match");
    sayln!(
        "      only to the player the backup came from (eMMC ID, or the serial for a wbrt backup);"
    );
    sayln!(
        "      any partition but contents (your music); writes only the 1 MiB pieces that differ,"
    );
    sayln!("      and reads every one back");
    sayln!();
    #[cfg(windows)]
    sayln!("  cinder-rescue driver                            install the USB driver (asks for administrator)");
    sayln!("  cinder-rescue licences                          the notices this program carries");
    sayln!();
    sayln!(
        "--da FILE uses your own Download Agent. Without it, DA.bin here or beside this program,"
    );
    sayln!("or else the one in wbrt's v1.0.9 release, downloaded and checked by SHA-256.");
    sayln!(
        "A dump writes FILE.manifest (chip, eMMC ID, SHA-256 per partition) and verifies itself."
    );
    sayln!("Only restore writes to the player.");
}

/// One command, from the terminal or from the page (`web`), so both do exactly the same.
fn run(args: &[String]) -> Result<(), String> {
    let opt = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .map(String::as_str)
    };
    let cmd = args.first().map(String::as_str).unwrap_or("");
    if cmd == "licences" {
        // The protocol facts are ported from an MIT tool, whose notice must travel with every copy;
        // libusb is linked statically under the LGPL 2.1 (its licence ships with each release).
        sayln!("{}", include_str!("../LICENSE-mediatek_flash_tool"));
        sayln!("libusb 1.0 is linked statically, under the GNU LGPL 2.1: https://libusb.info");
        sayln!("Source and relinking: build rescue/ from the tagged release with your own libusb.");
        return Ok(());
    }
    if cmd == "verify" {
        return verify(args.get(1).ok_or("verify needs a FILE")?);
    }
    #[cfg(windows)]
    if cmd == "driver" {
        return fetch::driver();
    }
    if !matches!(cmd, "info" | "dump" | "restore") {
        help();
        return Ok(());
    }
    let da = fetch::da(opt("--da"))?;
    if cmd == "info" {
        let (mut usb, _) = connect(&da)?;
        sayln!("Download mode works. Restarting the player.");
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
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args[0] == "gui" {
        // Double-clicked: the page. Anything it fails at is in the log it shows.
        if let Err(e) = web::serve() {
            eprintln!("cinder-rescue: {e}");
            std::process::exit(1);
        }
        return;
    }
    if let Err(e) = run(&args) {
        eprintln!("cinder-rescue: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtk::{sum16, ACK, READ_CHUNK};

    /// The DA's side of SWITCH_PART, READ and SDMMC_WRITE_DATA over an eMMC in RAM. Logs each write
    /// as (address, length); `flip` corrupts every written packet, as a failing eMMC would.
    struct Emmc {
        mem: Vec<u8>,
        inp: Vec<u8>,
        out: std::collections::VecDeque<u8>,
        state: State,
        writes: Vec<(usize, usize)>,
        flip: bool,
    }
    enum State {
        Cmd,
        Part,
        ReadSize(usize, usize),
        Reading(usize, usize),
        Writing(usize, usize),
    }

    impl Emmc {
        fn send_chunk(&mut self, pos: usize, end: usize) -> usize {
            let n = (end - pos).min(READ_CHUNK);
            let c = &self.mem[pos..pos + n];
            self.out.extend(c);
            self.out.extend(sum16(c).to_be_bytes());
            pos + n
        }
        fn step(&mut self) -> bool {
            let be = |b: &[u8]| u64::from_be_bytes(b.try_into().unwrap()) as usize;
            let (take, next) = match (&self.state, self.inp.first()) {
                (_, None) => return false,
                (State::Cmd, Some(0x60)) => {
                    self.out.push_back(ACK);
                    (1, State::Part)
                }
                (State::Part, Some(_)) => {
                    self.out.push_back(ACK);
                    (1, State::Cmd)
                }
                (State::Cmd, Some(0xd6)) if self.inp.len() >= 19 => {
                    self.out.push_back(ACK);
                    let (a, n) = (be(&self.inp[3..11]), be(&self.inp[11..19]));
                    (19, State::ReadSize(a, a + n))
                }
                (&State::ReadSize(a, e), _) if self.inp.len() >= 4 => {
                    (4, State::Reading(self.send_chunk(a, e), e))
                }
                (&State::Reading(p, e), Some(&ACK)) if p < e => {
                    (1, State::Reading(self.send_chunk(p, e), e))
                }
                (State::Reading(..), Some(&ACK)) => (1, State::Cmd),
                (State::Cmd, Some(0x62)) if self.inp.len() >= 23 => {
                    self.out.push_back(ACK);
                    let (a, n) = (be(&self.inp[3..11]), be(&self.inp[11..19]));
                    self.writes.push((a, n));
                    (23, State::Writing(a, a + n))
                }
                (&State::Writing(p, e), Some(&ACK)) => {
                    let n = (e - p).min(READ_CHUNK);
                    if self.inp.len() < 1 + n + 2 {
                        return false;
                    }
                    self.mem[p..p + n].copy_from_slice(&self.inp[1..1 + n]);
                    self.mem[p] ^= self.flip as u8;
                    self.out.push_back(0x69); // CONT_CHAR
                    (
                        1 + n + 2,
                        if p + n < e {
                            State::Writing(p + n, e)
                        } else {
                            State::Cmd
                        },
                    )
                }
                _ => return false,
            };
            self.inp.drain(..take);
            self.state = next;
            true
        }
    }
    impl mtk::Link for Emmc {
        fn read(&mut self, buf: &mut [u8]) -> Result<(), String> {
            for b in buf {
                *b = self.out.pop_front().ok_or("emulator ran dry")?;
            }
            Ok(())
        }
        fn write(&mut self, data: &[u8]) -> Result<(), String> {
            self.inp.extend_from_slice(data);
            while self.step() {}
            Ok(())
        }
        fn flush(&mut self) {}
    }

    #[test]
    fn restore_writes_only_the_pieces_that_differ_and_checks_them() {
        const START: usize = 0x100000;
        let size = 3 * PIECE + PIECE / 2; // piece 2 stays the same
        let want: Vec<u8> = (0..size).map(|i| (i % 253) as u8).collect();
        let emmc = |dev: &[u8], flip| Emmc {
            mem: [vec![0xee; START], dev.to_vec()].concat(),
            inp: vec![],
            out: Default::default(),
            state: State::Cmd,
            writes: vec![],
            flip,
        };
        let mut dev = want.clone();
        dev[10] ^= 1; // piece 0
        dev[PIECE + 7] ^= 1; // piece 1: one run with piece 0
        dev[size - 1] ^= 1; // the half piece at the end

        let mut e = emmc(&dev, false);
        let n = sync(
            &mut e,
            &mut &want[..],
            START as u64,
            size as u64,
            false,
            true,
            |_| {},
        );
        assert_eq!(n, Ok(size as u64 - PIECE as u64));
        assert!(
            e.writes.is_empty() && e.mem[START..] == dev[..],
            "dry run writes nothing"
        );

        let mut e = emmc(&dev, false);
        let n = sync(
            &mut e,
            &mut &want[..],
            START as u64,
            size as u64,
            false,
            false,
            |_| {},
        );
        assert_eq!(n, Ok(size as u64 - PIECE as u64));
        assert_eq!(
            e.writes,
            [(START, 2 * PIECE), (START + 3 * PIECE, PIECE / 2)]
        );
        assert!(e.mem[START..] == want[..] && e.mem[..START].iter().all(|&b| b == 0xee));

        let mut e = emmc(&want, false);
        assert_eq!(
            sync(
                &mut e,
                &mut &want[..],
                START as u64,
                size as u64,
                false,
                false,
                |_| {}
            ),
            Ok(0)
        );
        assert!(e.writes.is_empty(), "a matching partition is left alone");

        let mut e = emmc(&dev, true);
        let r = sync(
            &mut e,
            &mut &want[..],
            START as u64,
            size as u64,
            false,
            false,
            |_| {},
        );
        assert!(r.unwrap_err().contains("read back differently"));
    }
}
