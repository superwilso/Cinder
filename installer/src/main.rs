//! cinder-installer — pick which parts of Cinder to install, then stage them onto the Walkman.
//!
//! This is the artefact end users run. It is a console program on purpose: it has to work over
//! RDP, in a VM, from a rescue shell, and on a machine where nothing else is installed.
//!
//! WHAT IT DOES
//!   1. finds the player's storage (it mounts as a plain USB drive)
//!   2. asks which optional components to install — the catalogue is embedded from
//!      cinder-home/deploy/components.conf, so it can never drift from what the device installer
//!      actually understands
//!   3. writes the answers as cinder_components.conf and copies the staged files to the drive root
//!   4. writes the install package as NW_WM_FW.UPG
//!   5. tells the player to reboot into its updater, which applies the package
//!
//! HOW STEP 5 DIFFERS BY PLATFORM. On Windows it runs Sony's own `SoftwareUpdateTool.exe`, which
//! owns the whole handoff. On Linux it sends the same vendor SCSI command that tool ends with,
//! directly (see `trigger_fw_upgrade`) — this needs root. On macOS it cannot be sent at all and
//! the installer says so instead of inventing a step. There is NO update entry in the player's own
//! menus on this generation; a host has to trigger it.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

include!(concat!(env!("OUT_DIR"), "/payload.rs"));

const CONF_NAME: &str = "cinder_components.conf";

// ── the component catalogue ────────────────────────────────────────────────────────────────

#[derive(Clone)]
enum Kind {
    Bool,
    Enum(Vec<String>),
}

#[derive(Clone)]
struct Comp {
    id: String,
    var: String,
    kind: Kind,
    default: String,
    title: String,
    desc: String,
    value: String,
}

impl Comp {
    fn allowed(&self) -> Vec<String> {
        match &self.kind {
            Kind::Bool => vec!["0".into(), "1".into()],
            Kind::Enum(v) => v.clone(),
        }
    }
    fn valid(&self, v: &str) -> bool {
        self.allowed().iter().any(|a| a == v)
    }
    fn cycle(&mut self) {
        let a = self.allowed();
        let i = a.iter().position(|x| *x == self.value).unwrap_or(0);
        self.value = a[(i + 1) % a.len()].clone();
    }
    fn render(&self) -> String {
        match self.kind {
            Kind::Bool => if self.value == "1" { "[x]".into() } else { "[ ]".into() },
            Kind::Enum(_) => format!("<{}>", self.value),
        }
    }
}

/// Parse deploy/components.conf. Format: `id | VARNAME | type | default | title`, with indented
/// continuation lines forming the description. Same grammar tools/configure.sh reads.
fn parse_catalogue(text: &str) -> Result<Vec<Comp>, String> {
    let mut out: Vec<Comp> = Vec::new();
    for line in text.lines() {
        let t = line.trim_start();
        if t.starts_with('#') || t.is_empty() {
            continue;
        }
        let indented = line.starts_with(' ') || line.starts_with('\t');
        if !indented && line.contains('|') {
            let f: Vec<&str> = line.split('|').map(|s| s.trim()).collect();
            if f.len() < 5 {
                return Err(format!("malformed catalogue line: {line}"));
            }
            let kind = if f[2] == "bool" {
                Kind::Bool
            } else if let Some(rest) = f[2].strip_prefix("enum:") {
                Kind::Enum(rest.split(',').map(|s| s.trim().to_string()).collect())
            } else {
                return Err(format!("unknown component type '{}'", f[2]));
            };
            out.push(Comp {
                id: f[0].into(),
                var: f[1].into(),
                kind,
                default: f[3].into(),
                title: f[4].into(),
                desc: String::new(),
                value: f[3].into(),
            });
        } else if indented {
            if let Some(last) = out.last_mut() {
                last.desc.push_str(t);
                last.desc.push('\n');
            }
        }
    }
    if out.is_empty() {
        return Err("catalogue contained no components".into());
    }
    for c in &out {
        if !c.valid(&c.default) {
            return Err(format!("default '{}' invalid for '{}'", c.default, c.id));
        }
    }
    Ok(out)
}

// ── finding the player ─────────────────────────────────────────────────────────────────────

/// A Walkman's storage root has these at the top level. `DevIcon.fil` alone is a strong enough
/// signal; the MUSIC+PC_Application pair covers units where the icon file was deleted.
fn looks_like_walkman(root: &Path) -> bool {
    let has = |n: &str| root.join(n).exists();
    has("DevIcon.fil") || (has("MUSIC") && has("PC_Application"))
}

fn candidate_roots() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if cfg!(windows) {
        for c in b'A'..=b'Z' {
            v.push(PathBuf::from(format!("{}:\\", c as char)));
        }
    } else {
        // Linux/macOS: wherever removable media gets mounted.
        for base in ["/media", "/run/media", "/mnt", "/Volumes"] {
            if let Ok(rd) = fs::read_dir(base) {
                for e in rd.flatten() {
                    let p = e.path();
                    if p.is_dir() {
                        v.push(p.clone());
                        if let Ok(inner) = fs::read_dir(&p) {
                            for e2 in inner.flatten() {
                                if e2.path().is_dir() {
                                    v.push(e2.path());
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    v
}

fn find_players() -> Vec<PathBuf> {
    candidate_roots()
        .into_iter()
        .filter(|p| p.is_dir() && looks_like_walkman(p))
        .collect()
}

// ── console helpers ────────────────────────────────────────────────────────────────────────

fn prompt(msg: &str) -> String {
    print!("{msg}");
    let _ = io::stdout().flush();
    let mut s = String::new();
    if io::stdin().read_line(&mut s).is_err() {
        return String::new();
    }
    s.trim().to_string()
}

fn show(comps: &[Comp], target: &Path) {
    println!();
    println!("  Cinder installer  (channel: {CHANNEL})");
    println!("  player: {}", target.display());
    println!("  ------------------------------------------------------------");
    for (i, c) in comps.iter().enumerate() {
        println!("   {:>2}  {:<7} {:<42} {}", i + 1, c.render(), c.title, c.id);
    }
    println!("  ------------------------------------------------------------");
    println!("\n  What the options do:");
    for (i, c) in comps.iter().enumerate() {
        println!("\n   {}. {}  [default: {}]", i + 1, c.title, c.default);
        for line in c.desc.lines() {
            println!("      {line}");
        }
    }
    println!("\n   <number> toggle/cycle   ?<number> repeat one description   i install   q quit");
}

// ── writing ────────────────────────────────────────────────────────────────────────────────

fn conf_text(comps: &[Comp]) -> String {
    let mut s = String::new();
    s.push_str("# cinder_components.conf - generated by cinder-installer; do not edit by hand.\n");
    s.push_str("# Read (never sourced) by install_cinderhome.sh on the device.\n");
    s.push_str(&format!("# channel: {CHANNEL}\n"));
    for c in comps {
        s.push_str(&format!("\n# {}\n{}={}\n", c.title, c.var, c.value));
    }
    s
}

/// Is the component that owns this payload file switched on? An empty owner means the file is
/// part of every install (the app itself, the probe, the .UPG). A bool component set to "0" is
/// off; an enum component is always staged, because its switcher has to be present on the device
/// for the choice to be changeable later.
fn selected(comps: &[Comp], owner: &str) -> bool {
    if owner.is_empty() {
        return true;
    }
    comps
        .iter()
        .find(|c| c.id == owner)
        .map(|c| c.value != "0")
        .unwrap_or(true)
}

fn staged_count(comps: &[Comp]) -> usize {
    PAYLOAD.iter().filter(|(_, o, _)| selected(comps, o)).count()
}

fn install(comps: &[Comp], target: &Path) -> io::Result<()> {
    println!("\n  writing to {} ...", target.display());
    for (name, owner, bytes) in PAYLOAD {
        if !selected(comps, owner) {
            println!("    {:<22} {:>9}", name, "not selected");
            continue;
        }
        let dest = target.join(name);
        fs::write(&dest, bytes)?;
        println!("    {:<22} {:>9} bytes", name, bytes.len());
    }
    let conf = target.join(CONF_NAME);
    fs::write(&conf, conf_text(comps))?;
    println!("    {:<22} {:>9} bytes", CONF_NAME, conf_text(comps).len());
    Ok(())
}

/// Launch Sony's own Windows updater from an embedded temporary bundle. It owns the device
/// handoff: do not manually eject or trigger a Linux SCSI command before this returns.
#[cfg(windows)]
fn run_sony_updater() -> io::Result<()> {
    use std::process::Command;
    let root = std::env::temp_dir().join(format!("cinder-updater-{}", std::process::id()));
    if root.exists() {
        fs::remove_dir_all(&root)?;
    }
    for (relative, bytes) in UPDATER_PAYLOAD {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, bytes)?;
    }
    let upg = PAYLOAD
        .iter()
        .find(|(name, _, _)| *name == "NW_WM_FW.UPG")
        .map(|(_, _, bytes)| *bytes)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "embedded install UPG missing"))?;
    fs::write(root.join("Data/Device/NW_WM_FW.UPG"), upg)?;
    let exe = root.join("SoftwareUpdateTool.exe");
    println!("\n  Starting Sony's firmware updater...");
    let status = Command::new(&exe).current_dir(&root).status()?;
    fs::remove_dir_all(&root).ok();
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::Other,
            format!("Sony updater exited with {status}"),
        ))
    }
}

// ── the firmware-upgrade trigger, off Windows ──────────────────────────────────────────────
//
// WHAT SONY'S TOOL ACTUALLY DOES. `SoftwareUpdateTool.exe` is not magic and it is not a driver:
// its last act is a single 12-byte vendor SCSI command telling the player to reboot into its
// updater, which then finds NW_WM_FW.UPG on the data partition and applies it. The command is
// documented by Rockbox's `nwztools/scsitool` (`try_fw_upgrade`) and this project has been
// sending it from Linux for months — `tools/flash.sh` ends in exactly this, and that is how every
// development build gets flashed.
//
// So the old claim in this file — "there is no Linux or macOS equivalent" — was simply wrong, and
// the instruction it printed instead ("Settings > Device Settings > Update") was worse than wrong:
// THE NW-A55 HAS NO SUCH MENU ENTRY. Updates on this generation are driven entirely from the host.
// A user following those steps would hunt for a button that does not exist, on a device already
// holding a correctly staged payload.
//
// WHY NOT SHELL OUT TO scsitool: it is a build-it-yourself binary from a vendored Rockbox
// checkout. The whole point of this installer is that it is one file an end user can run.

/// The vendor CDB. `fc` is Sony's NWZ passthrough opcode; subcommand `04` + the `dbmn` tag is
/// "do firmware upgrade". Byte 8 is a flag: newer devices want 0x80, older ones 0x00, so the
/// caller tries 0x80 first and falls back — the same two-shot `do_fw_upgrade` does.
#[cfg(target_os = "linux")]
const FW_UPGRADE_CDB: [u8; 12] = [0xfc, 0, 0x04, b'd', b'b', b'm', b'n', 0, 0, 0, 0, 0];

/// Resolve a mount point to the block device backing it, via /proc/self/mountinfo.
///
/// Field 5 is the mount point and the source follows the " - " separator. Path fields are escaped
/// with octal for space, tab, newline and backslash, so a Walkman mounted at `/media/me/WALKMAN 1`
/// appears as `WALKMAN\0401` and a naive comparison misses it.
#[cfg(target_os = "linux")]
fn block_device_for(mount: &Path) -> Option<PathBuf> {
    let text = fs::read_to_string("/proc/self/mountinfo").ok()?;
    block_device_in(&text, mount)
}

/// The parsing half of [`block_device_for`], split out so it can be tested against a captured
/// mountinfo rather than whatever happens to be mounted on the machine running the suite.
#[cfg(target_os = "linux")]
fn block_device_in(text: &str, mount: &Path) -> Option<PathBuf> {
    fn unescape(s: &str) -> String {
        let b = s.as_bytes();
        let mut out = String::with_capacity(s.len());
        let mut i = 0;
        while i < b.len() {
            if b[i] == b'\\' && i + 3 < b.len() {
                let d = &s[i + 1..i + 4];
                if let Ok(v) = u8::from_str_radix(d, 8) {
                    out.push(v as char);
                    i += 4;
                    continue;
                }
            }
            out.push(b[i] as char);
            i += 1;
        }
        out
    }
    for line in text.lines() {
        let Some((before, after)) = line.split_once(" - ") else {
            continue;
        };
        let f: Vec<&str> = before.split_whitespace().collect();
        if f.len() < 5 {
            continue;
        }
        if Path::new(&unescape(f[4])) != mount {
            continue;
        }
        let Some(src) = after.split_whitespace().nth(1) else {
            continue;
        };
        if src.starts_with('/') {
            return Some(PathBuf::from(src));
        }
    }
    None
}

#[cfg(target_os = "linux")]
mod sg {
    use std::os::raw::{c_int, c_ulong, c_void};

    pub const SG_IO: c_ulong = 0x2285;
    pub const SG_DXFER_FROM_DEV: c_int = -3;

    #[repr(C)]
    pub struct SgIoHdr {
        pub interface_id: c_int,
        pub dxfer_direction: c_int,
        pub cmd_len: u8,
        pub mx_sb_len: u8,
        pub iovec_count: u16,
        pub dxfer_len: u32,
        pub dxferp: *mut c_void,
        pub cmdp: *const u8,
        pub sbp: *mut u8,
        pub timeout: u32,
        pub flags: u32,
        pub pack_id: c_int,
        pub usr_ptr: *mut c_void,
        pub status: u8,
        pub masked_status: u8,
        pub msg_status: u8,
        pub sb_len_wr: u8,
        pub host_status: u16,
        pub driver_status: u16,
        pub resid: c_int,
        pub duration: u32,
        pub info: u32,
    }

    extern "C" {
        pub fn ioctl(fd: c_int, request: c_ulong, arg: *mut c_void) -> c_int;
        pub fn sync();
        pub fn umount(target: *const u8) -> c_int;
    }
}

/// Fire the vendor command at `dev` with one flag byte. Ok(()) means the drive accepted it.
#[cfg(target_os = "linux")]
fn send_fw_upgrade(dev: &Path, flag: u8) -> io::Result<()> {
    use std::os::unix::io::AsRawFd;

    let f = fs::OpenOptions::new().read(true).write(true).open(dev)?;
    let mut cdb = FW_UPGRADE_CDB;
    cdb[8] = flag;
    let mut buf = [0u8; 0x80];
    let mut sense = [0u8; 32];

    let mut h = sg::SgIoHdr {
        interface_id: i32::from(b'S'),
        dxfer_direction: sg::SG_DXFER_FROM_DEV,
        cmd_len: cdb.len() as u8,
        mx_sb_len: sense.len() as u8,
        iovec_count: 0,
        dxfer_len: buf.len() as u32,
        dxferp: buf.as_mut_ptr().cast(),
        cmdp: cdb.as_ptr(),
        sbp: sense.as_mut_ptr(),
        timeout: 30_000,
        flags: 0,
        pack_id: 0,
        usr_ptr: std::ptr::null_mut(),
        status: 0,
        masked_status: 0,
        msg_status: 0,
        sb_len_wr: 0,
        host_status: 0,
        driver_status: 0,
        resid: 0,
        duration: 0,
        info: 0,
    };

    // SAFETY: h outlives the call; every pointer in it addresses a live local buffer whose
    // declared length matches the field beside it.
    let rc = unsafe { sg::ioctl(f.as_raw_fd(), sg::SG_IO, (&mut h as *mut sg::SgIoHdr).cast()) };
    if rc < 0 {
        return Err(io::Error::last_os_error());
    }
    // A non-zero SCSI status means the drive understood the transport and rejected the command —
    // that is the signal to try the other flag byte, not to fail the install.
    if h.status != 0 || h.host_status != 0 || h.driver_status != 0 {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!(
                "drive rejected the upgrade command (status {:#x}, host {:#x}, driver {:#x})",
                h.status, h.host_status, h.driver_status
            ),
        ));
    }
    Ok(())
}

/// Flush, unmount, then tell the player to reboot into its updater.
///
/// ORDER MATTERS. The payload has just been written through the page cache; if the device reboots
/// before that reaches the flash, the updater looks for NW_WM_FW.UPG and finds a truncated file or
/// no file at all. `tools/flash.sh` syncs and unmounts before it fires for exactly this reason.
/// The unmount is best-effort — a desktop file manager may hold the mount, and a synced-but-
/// mounted device still updates correctly; a device that never got the bytes does not.
#[cfg(target_os = "linux")]
fn trigger_fw_upgrade(target: &Path) -> io::Result<()> {
    let dev = block_device_for(target).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("could not find the block device behind {}", target.display()),
        )
    })?;

    // SAFETY: sync() takes no arguments and cannot fail.
    unsafe { sg::sync() };

    let mut c: Vec<u8> = target.as_os_str().as_encoded_bytes().to_vec();
    c.push(0);
    // SAFETY: c is NUL-terminated and lives across the call.
    unsafe { sg::umount(c.as_ptr()) };

    println!("\n  Telling the player to reboot into Sony's updater ({})...", dev.display());
    match send_fw_upgrade(&dev, 0x80) {
        Ok(()) => Ok(()),
        Err(first) => {
            println!("  (newer-style command refused: {first} — trying the older one)");
            send_fw_upgrade(&dev, 0x00)
        }
    }
}

/// macOS has no SG_IO. Raw SCSI passthrough there needs an IOKit `SCSITaskUserClient`, and the
/// kernel will not hand one over for a disk it has already claimed and mounted — which is exactly
/// the state a staged Walkman is in. Rather than pretend, the macOS build stages and says so.
#[cfg(all(unix, not(target_os = "linux")))]
fn trigger_fw_upgrade(_target: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "macOS cannot send the vendor SCSI command",
    ))
}


fn finish_install(comps: &[Comp], target: &Path) -> ! {
    if let Err(e) = install(comps, target) {
        eprintln!("\nERROR: {e}");
        eprintln!("The player may now hold a partial copy. Re-run before updating.");
        std::process::exit(1);
    }
    #[cfg(windows)]
    {
        println!("\n  Files staged. Sony's updater will now take over the USB connection.");
        if let Err(e) = run_sony_updater() {
            eprintln!("\nERROR: could not start the Sony updater: {e}");
            eprintln!("The files are staged; reconnect the Walkman and run this installer again.");
            std::process::exit(1);
        }
        print_next_steps();
    }
    // Off Windows the trigger is ours to send. A failure is reported and then explained rather
    // than exited on: the payload is staged and valid either way, and the user's next move
    // depends on WHY it failed (not root, device unmounted by a file manager, macOS at all).
    #[cfg(not(windows))]
    {
        println!("\n  Files staged.");
        match trigger_fw_upgrade(target) {
            Ok(()) => print_upgrade_sent(),
            Err(e) => print_trigger_failed(&e),
        }
    }
    std::process::exit(0);
}

// ── main ───────────────────────────────────────────────────────────────────────────────────

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut explicit: Option<PathBuf> = None;
    let mut assume_yes = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-y" | "--yes" => assume_yes = true,
            "-h" | "--help" => {
                println!("cinder-installer [--yes] [<drive or mount point>]");
                println!("  Stages Cinder onto the player, then triggers its firmware updater.");
                if cfg!(windows) {
                    println!("  Runs Sony's SoftwareUpdateTool.exe for the USB handoff and reboot.");
                } else if cfg!(target_os = "linux") {
                    println!("  Sends the upgrade command directly — NEEDS ROOT (run with sudo).");
                } else {
                    println!("  This platform can stage files but cannot send the upgrade command;");
                    println!("  finish from Linux or Windows. The player has no update menu.");
                }
                return;
            }
            other => explicit = Some(PathBuf::from(other)),
        }
        i += 1;
    }

    println!("Cinder installer — channel {CHANNEL}");

    // NO PLATFORM GATE HERE. There used to be one — `if !cfg!(windows) { exit(2) }` — and it made
    // the published Linux artefact unrunnable: the release workflow has been building, testing and
    // uploading `cinder-installer-linux-x64` on every push, and the first thing it did on a user's
    // machine was refuse to start. Every `#[cfg(not(windows))]` path below it was dead code that
    // could never execute. The Linux build now does the whole job; see `trigger_fw_upgrade`.
    if !MISSING.is_empty() {
        eprintln!("\nERROR: this build is incomplete — the following payload files were missing");
        eprintln!("when it was compiled:");
        for m in MISSING {
            eprintln!("    {m}");
        }
        eprintln!("\nBuild them first:  cinder-home/build.sh {CHANNEL}");
        eprintln!("                   cinder-home/tools/pack_upg.sh {CHANNEL}");
        std::process::exit(2);
    }
    if !UPDATER_MISSING.is_empty() {
        eprintln!("\nERROR: the embedded Sony Windows updater is incomplete:");
        for m in UPDATER_MISSING {
            eprintln!("    {m}");
        }
        eprintln!("This release must be built from the authorized updater bundle.");
        std::process::exit(2);
    }

    let catalogue = match CATALOGUE {
        Some(c) => c,
        None => {
            eprintln!("ERROR: no component catalogue embedded (deploy/components.conf missing).");
            std::process::exit(2);
        }
    };
    let mut comps = match parse_catalogue(catalogue) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("ERROR: {e}");
            std::process::exit(2);
        }
    };

    // ── locate the player ──
    let target = match explicit {
        Some(p) => {
            if !p.is_dir() {
                eprintln!("ERROR: {} is not a directory.", p.display());
                std::process::exit(1);
            }
            if !looks_like_walkman(&p) {
                eprintln!("WARNING: {} does not look like a Walkman's storage", p.display());
                eprintln!("         (no DevIcon.fil, no MUSIC + PC_Application).");
                if !assume_yes && prompt("         Use it anyway? [y/N] ").to_lowercase() != "y" {
                    return;
                }
            }
            p
        }
        None => {
            let found = find_players();
            match found.len() {
                0 => {
                    eprintln!("\nERROR: no Walkman found.");
                    eprintln!("  Connect the player by USB and set it to mass-storage mode, then");
                    eprintln!("  run this again. If it is mounted somewhere unusual, pass the path:");
                    // Was hardcoded to `D:\` — a Windows drive letter, printed on Linux and
                    // macOS too, where it is not a path anyone can type.
                    if cfg!(windows) {
                        eprintln!("      cinder-installer D:\\");
                    } else {
                        eprintln!("      cinder-installer /media/you/WALKMAN");
                    }
                    std::process::exit(1);
                }
                1 => found[0].clone(),
                _ => {
                    println!("\nSeveral candidates found:");
                    for (n, p) in found.iter().enumerate() {
                        println!("   {}  {}", n + 1, p.display());
                    }
                    let a = prompt("Which one? ");
                    match a.parse::<usize>() {
                        Ok(n) if n >= 1 && n <= found.len() => found[n - 1].clone(),
                        _ => {
                            eprintln!("Not a listed choice — stopping.");
                            std::process::exit(1);
                        }
                    }
                }
            }
        }
    };

    // ── pick ──
    if assume_yes {
        finish_install(&comps, &target);
    }

    loop {
        show(&comps, &target);
        let a = prompt("  > ");
        if a.is_empty() {
            continue;
        }
        if a == "q" || a == "Q" {
            println!("  nothing written.");
            return;
        }
        if a == "i" || a == "I" {
            break;
        }
        if let Some(rest) = a.strip_prefix('?') {
            if let Ok(n) = rest.trim().parse::<usize>() {
                if n >= 1 && n <= comps.len() {
                    let c = &comps[n - 1];
                    println!("\n  {}  ({} -> {})", c.title, c.id, c.var);
                    println!("  allowed: {}   default: {}", c.allowed().join(" "), c.default);
                    println!();
                    for l in c.desc.lines() {
                        println!("    {l}");
                    }
                }
            }
            continue;
        }
        if let Ok(n) = a.parse::<usize>() {
            if n >= 1 && n <= comps.len() {
                comps[n - 1].cycle();
            }
        }
    }

    // ── confirm, then write ──
    println!("\n  About to write to {}:", target.display());
    for c in &comps {
        println!("    {:<10} {}", c.id, c.value);
    }
    println!(
        "\n  {} files plus {} will be copied to the player's storage root.",
        staged_count(&comps),
        CONF_NAME
    );
    println!("  Nothing is flashed by this program — the player does that itself, later.");
    if prompt("\n  Proceed? [y/N] ").to_lowercase() != "y" {
        println!("  nothing written.");
        return;
    }

    finish_install(&comps, &target);
}

#[cfg(windows)]
fn print_next_steps() {
    println!("\n  Sony's updater has finished. The Walkman should reboot into Cinder.");
    println!("\n  If a boot ever goes wrong: hold the player's USB cable in at power-on to get");
    println!("  the stock player back, and see RECOVERY.md.");
}

/// The command went out and the player is rebooting into the updater on its own.
#[cfg(not(windows))]
fn print_upgrade_sent() {
    println!("\n  Upgrade command accepted. The player is rebooting into Sony's updater.");
    println!();
    println!("    * The screen shows the updater, then it reboots into Cinder by itself.");
    println!("    * It DROPS OFF USB while it works. That is expected — leave the cable in and");
    println!("      do not touch it until the player comes back on its own.");
    recovery_note();
}

/// The staging worked and the trigger did not. Which of those two happened decides what the user
/// should do next, so say which, and say why — this is the path that used to print instructions
/// for a menu the NW-A55 does not have.
#[cfg(not(windows))]
fn print_trigger_failed(e: &io::Error) {
    println!("\n  Every file is staged on the player, including NW_WM_FW.UPG.");
    println!("  What did NOT happen is the last step: telling it to reboot into its updater.");
    println!("\n    reason: {e}");
    println!();
    if e.kind() == io::ErrorKind::PermissionDenied {
        println!("  That is a permissions error. Sending a raw SCSI command needs root:");
        println!();
        println!("      sudo {}", std::env::args().next().unwrap_or_else(|| "cinder-installer".into()));
        println!();
        println!("  Re-running it is safe — it stages the same files again, then fires.");
    } else if cfg!(target_os = "macos") {
        println!("  macOS cannot send this command at all. It is a vendor SCSI passthrough, and");
        println!("  macOS only exposes that through an IOKit SCSITaskUserClient, which the kernel");
        println!("  refuses for a disk it has already mounted — the exact state the player is in.");
        println!();
        println!("  Finish from a Linux or Windows machine. THE STAGING IS ALREADY DONE, so plug");
        println!("  the player into one and run the installer there; it will re-stage and fire.");
    } else {
        println!("  The player is still holding a valid payload, so nothing is broken. Re-running");
        println!("  this installer is safe. If it keeps failing, a file manager may be holding the");
        println!("  mount open — eject the player in your desktop, plug it back in, and retry.");
    }
    println!();
    println!("  DO NOT go looking for an update option on the player itself. This generation has");
    println!("  no such menu; the upgrade is always triggered by the host over USB.");
    recovery_note();
}

#[cfg(not(windows))]
fn recovery_note() {
    println!("\n  If a boot ever goes wrong: hold the player's USB cable in at power-on to get");
    println!("  the stock player back, and see RECOVERY.md.");
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
# a comment
power | CINDER_POWER | bool | 1 | Power off / Restart menu
    Installs cinder-power.
    Second line.

signature | CINDER_SIGNATURE | enum:stock,pv1,pv2 | stock | Audio sound signature
    Patches three bytes.
";

    #[test]
    fn parses_both_kinds() {
        let c = parse_catalogue(SAMPLE).unwrap();
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].id, "power");
        assert_eq!(c[0].var, "CINDER_POWER");
        assert!(matches!(c[0].kind, Kind::Bool));
        assert_eq!(c[0].value, "1");
        assert_eq!(c[1].allowed(), vec!["stock", "pv1", "pv2"]);
    }

    #[test]
    fn description_lines_attach_to_owner() {
        let c = parse_catalogue(SAMPLE).unwrap();
        assert!(c[0].desc.contains("Installs cinder-power."));
        assert!(c[0].desc.contains("Second line."));
        assert!(!c[0].desc.contains("Patches three bytes."));
        assert!(c[1].desc.contains("Patches three bytes."));
    }

    #[test]
    fn cycle_wraps_and_stays_valid() {
        let mut c = parse_catalogue(SAMPLE).unwrap();
        let sig = &mut c[1];
        assert_eq!(sig.value, "stock");
        sig.cycle();
        assert_eq!(sig.value, "pv1");
        sig.cycle();
        assert_eq!(sig.value, "pv2");
        sig.cycle();
        assert_eq!(sig.value, "stock");
        assert!(sig.valid(&sig.value));
    }

    #[test]
    fn bool_toggles() {
        let mut c = parse_catalogue(SAMPLE).unwrap();
        assert_eq!(c[0].render(), "[x]");
        c[0].cycle();
        assert_eq!(c[0].value, "0");
        assert_eq!(c[0].render(), "[ ]");
    }

    #[test]
    fn generated_conf_is_parseable_key_values() {
        let c = parse_catalogue(SAMPLE).unwrap();
        let t = conf_text(&c);
        assert!(t.contains("CINDER_POWER=1"));
        assert!(t.contains("CINDER_SIGNATURE=stock"));
        // every non-comment, non-blank line must be a bare KEY=VALUE — the device side greps
        // for exactly that shape and ignores anything else.
        for line in t.lines().filter(|l| !l.starts_with('#') && !l.trim().is_empty()) {
            let (k, v) = line.split_once('=').expect("KEY=VALUE");
            assert!(k.chars().all(|ch| ch.is_ascii_uppercase() || ch == '_'), "{k}");
            assert!(v.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_'), "{v}");
        }
    }

    // ── the off-Windows firmware trigger ───────────────────────────────────────────────────
    //
    // These cover the parsing and the command bytes. What they CANNOT cover is the SCSI exchange
    // itself: that needs a Walkman in MSC mode and it reboots the device into its updater, so it
    // is a hardware step, tracked in the device checklist rather than faked here.

    #[cfg(target_os = "linux")]
    const MOUNTINFO: &str = "\
25 30 0:23 / /proc rw,relatime shared:5 - proc proc rw
41 30 8:1 / /boot rw,relatime shared:9 - ext4 /dev/sda1 rw
77 44 8:33 / /media/me/WALKMAN rw,nosuid,nodev,relatime shared:61 - vfat /dev/sdc1 rw,uid=1000
88 44 8:49 / /media/me/MY\\040PLAYER rw,relatime shared:63 - vfat /dev/sdd1 rw,uid=1000
99 30 0:52 / /run/user/1000/doc rw,nosuid,nodev,relatime shared:70 - fuse.portal portal rw";

    #[test]
    #[cfg(target_os = "linux")]
    fn finds_the_block_device_behind_a_mount() {
        assert_eq!(
            block_device_in(MOUNTINFO, Path::new("/media/me/WALKMAN")),
            Some(PathBuf::from("/dev/sdc1"))
        );
    }

    /// mountinfo octal-escapes spaces. A Walkman labelled "MY PLAYER" mounts at a path containing
    /// one, and comparing the raw field would silently miss it — the installer would then report
    /// "could not find the block device" for a perfectly ordinary drive.
    #[test]
    #[cfg(target_os = "linux")]
    fn decodes_octal_escapes_in_the_mount_point() {
        assert_eq!(
            block_device_in(MOUNTINFO, Path::new("/media/me/MY PLAYER")),
            Some(PathBuf::from("/dev/sdd1"))
        );
    }

    /// Pseudo-filesystems have a source that is not a path. Returning "proc" or "portal" as a
    /// block device would send the ioctl to something that is not a drive.
    #[test]
    #[cfg(target_os = "linux")]
    fn ignores_sources_that_are_not_block_devices() {
        assert_eq!(block_device_in(MOUNTINFO, Path::new("/proc")), None);
        assert_eq!(block_device_in(MOUNTINFO, Path::new("/run/user/1000/doc")), None);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn unknown_mount_point_is_not_a_guess() {
        assert_eq!(block_device_in(MOUNTINFO, Path::new("/media/me/NOPE")), None);
    }

    /// The exact bytes Rockbox's scsitool sends (`try_fw_upgrade`): opcode 0xfc, subcommand 0x04,
    /// the "dbmn" tag, and the flag byte the caller varies. If this drifts, the player either
    /// ignores the command or does something else entirely.
    #[test]
    #[cfg(target_os = "linux")]
    fn the_upgrade_cdb_matches_the_documented_command() {
        assert_eq!(
            FW_UPGRADE_CDB,
            [0xfc, 0, 0x04, b'd', b'b', b'm', b'n', 0, 0, 0, 0, 0]
        );
        let mut with_flag = FW_UPGRADE_CDB;
        with_flag[8] = 0x80;
        assert_eq!(with_flag[8], 0x80, "the flag byte is index 8");
        assert_eq!(&with_flag[..8], &FW_UPGRADE_CDB[..8], "only byte 8 varies");
    }

    #[test]
    fn rejects_bad_catalogue() {
        assert!(parse_catalogue("").is_err());
        assert!(parse_catalogue("a | B | wat | 1 | T").is_err());
        assert!(parse_catalogue("a | B | bool | 7 | T").is_err());
    }

    #[test]
    fn walkman_detection_needs_the_markers() {
        let tmp = std::env::temp_dir().join(format!("cinder-inst-test-{}", std::process::id()));
        let _ = fs::create_dir_all(tmp.join("MUSIC"));
        assert!(!looks_like_walkman(&tmp), "MUSIC alone is not enough");
        let _ = fs::create_dir_all(tmp.join("PC_Application"));
        assert!(looks_like_walkman(&tmp));
        let _ = fs::remove_dir_all(&tmp);
    }
}
