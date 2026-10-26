//! cinder-rescue — back up a Sony NW-A50-series Walkman's eMMC over MediaTek's download mode.
//!
//! Download mode lives in the preloader, before any kernel, so it still answers on a player that
//! boot-loops. This first version only READS: `info` proves the link, `dump` copies a range of the
//! eMMC user area to a new file. No command that writes to the player exists in this program.
//!
//! The Download Agent (DA) is MediaTek's and is not shipped here; pass the one you already have
//! (wbrt carries `DA.bin`; the flash tool's README names `MTK_AllInOne_DA_5.2136.bin`).
//!
//! Windows needs a WinUSB driver on VID_0E8D&PID_2000 ("MT65xx Preloader"), the one wbrt installs.

mod mtk;

use mtk::Link;
use std::io::Write;
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

/// Handshake, load the DA, and leave stage 2 running. Prints what it learns.
fn connect(da: &[u8]) -> Result<Usb, String> {
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
    Ok(usb)
}

fn dump(da: &[u8], out: &str, addr: u64, len: u64) -> Result<(), String> {
    // A new file only: never overwrite a backup that already exists. Checked before the wait,
    // created after it, so a run that never connects leaves nothing behind.
    if std::path::Path::new(out).exists() {
        return Err(format!("{out} already exists; pick a new name"));
    }
    let mut usb = connect(da)?;
    let file = std::fs::OpenOptions::new().write(true).create_new(true).open(out).map_err(|e| format!("{out}: {e}"))?;
    let mut file = std::io::BufWriter::with_capacity(1 << 20, file);
    println!("Reading {len} bytes from {addr:#x} into {out}");
    let start = Instant::now();
    let mut done = 0u64;
    let mut shown = 0u64;
    let res = mtk::read_user(&mut usb, addr, len, |chunk| {
        file.write_all(chunk).map_err(|e| format!("{out}: {e}"))?;
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
    file.flush().map_err(|e| format!("{out}: {e}"))?;
    println!("\nDone in {:.0} s. Restarting the player.", start.elapsed().as_secs_f64());
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
    println!();
    println!("Read-only: nothing here writes to the player.");
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let opt = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(String::as_str);
    let cmd = args.first().map(String::as_str).unwrap_or("");
    if !matches!(cmd, "info" | "dump") {
        help();
        return Ok(());
    }
    let da_path = opt("--da").ok_or("--da DA.bin is required")?;
    let da = std::fs::read(da_path).map_err(|e| format!("{da_path}: {e}"))?;
    if cmd == "info" {
        let mut usb = connect(&da)?;
        println!("Download mode works. Restarting the player.");
        return mtk::reboot(&mut usb);
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
