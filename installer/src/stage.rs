//! Writing the payload to the player, and telling it to flash.
//!
//! Nothing here flashes anything itself. Every action is the same two steps: copy files to the
//! player's storage root, then send the one command that makes the player reboot into Sony's
//! updater, which finds `NW_WM_FW.UPG` and applies it. Install and uninstall differ ONLY in which
//! `.UPG` gets written under that name.

use std::fmt;
use std::fs;
use std::io;
use std::path::Path;
#[cfg(target_os = "linux")]
use std::path::PathBuf;

use crate::catalogue::Comp;
use crate::device::CONF_NAME;

/// Which package to stage. The device does the rest.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    /// Fresh install: write every selected payload file plus the component conf.
    Install,
    /// Update: identical mechanics to Install, and deliberately so — the device install script is
    /// idempotent and re-running it is how a new build lands. The difference is upstream, in the
    /// UI: an update starts from the answers already on the player instead of the defaults.
    Update,
    /// Uninstall: write ONLY the uninstall `.UPG`. No binaries, no conf — the package's whole job
    /// is to restore Sony's `.appcfg` and delete what the install put on `/system`.
    Uninstall,
}

impl Action {
    pub fn verb(self) -> &'static str {
        match self {
            Action::Install => "Install",
            Action::Update => "Update",
            Action::Uninstall => "Uninstall",
        }
    }

    pub fn is_removal(self) -> bool {
        self == Action::Uninstall
    }
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.verb())
    }
}

/// The name the package MUST land under. The device's own update flow looks for exactly this.
pub const UPG_NAME: &str = "NW_WM_FW.UPG";

/// Is the component that owns this payload file switched on? An empty owner means the file is
/// part of every install (the app itself, the probe, the `.UPG`). A bool component set to "0" is
/// off; an enum component is always staged, because its switcher has to be present on the device
/// for the choice to be changeable later.
pub fn selected(comps: &[Comp], owner: &str) -> bool {
    if owner.is_empty() {
        return true;
    }
    comps.iter().find(|c| c.id == owner).map(Comp::is_on).unwrap_or(true)
}

/// What an action will write, resolved before a single byte is copied.
///
/// Split out from the copying so the GUI can show an accurate file-by-file plan on the confirm
/// screen, and so the plan itself is testable without a Walkman plugged in.
pub struct Plan {
    pub files: Vec<(String, usize)>,
    pub conf: Option<String>,
}

impl Plan {
    pub fn total_bytes(&self) -> usize {
        self.files.iter().map(|(_, n)| n).sum::<usize>()
            + self.conf.as_ref().map(String::len).unwrap_or(0)
    }

    pub fn count(&self) -> usize {
        self.files.len() + usize::from(self.conf.is_some())
    }
}

/// Build the plan for `action`. Returns Err when the build cannot carry it out at all — the only
/// case being an uninstall from a binary whose payload has no uninstall package embedded.
pub fn plan(action: Action, comps: &[Comp], channel: &str) -> Result<Plan, String> {
    if action.is_removal() {
        let bytes = crate::payload::UNINSTALL_UPG.ok_or_else(|| {
            "this build has no uninstall package embedded (cinder_home_uninstall.upg was missing \
             when it was compiled)"
                .to_string()
        })?;
        return Ok(Plan { files: vec![(UPG_NAME.to_string(), bytes.len())], conf: None });
    }

    let files = crate::payload::PAYLOAD
        .iter()
        .filter(|(_, owner, _)| selected(comps, owner))
        .map(|(name, _, bytes)| ((*name).to_string(), bytes.len()))
        .collect();
    Ok(Plan { files, conf: Some(crate::catalogue::conf_text(comps, channel)) })
}

/// Copy the plan onto the player. `progress` is called after each file with (name, bytes).
///
/// `dry` walks the whole plan and writes NOTHING. It exists so the interface can be exercised
/// end to end — on a real player, with a real component selection — without putting a package on
/// the device or triggering a flash. A GUI that can only be tested by flashing a Walkman is a GUI
/// that does not get tested.
pub fn write_payload(
    action: Action,
    comps: &[Comp],
    channel: &str,
    target: &Path,
    dry: bool,
    mut progress: impl FnMut(&str, usize),
) -> io::Result<()> {
    if action.is_removal() {
        let bytes = crate::payload::UNINSTALL_UPG
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no uninstall package embedded"))?;
        if !dry {
            write_verified(&target.join(UPG_NAME), bytes)?;
        }
        progress(UPG_NAME, bytes.len());
        return Ok(());
    }

    for (name, owner, bytes) in crate::payload::PAYLOAD {
        if !selected(comps, owner) {
            continue;
        }
        if !dry {
            write_verified(&target.join(name), bytes)?;
        }
        progress(name, bytes.len());
    }
    let conf = crate::catalogue::conf_text(comps, channel);
    if !dry {
        write_verified(&target.join(CONF_NAME), conf.as_bytes())?;
    }
    progress(CONF_NAME, conf.len());
    Ok(())
}

/// Write, then read back and compare.
///
/// The target is a FAT32 volume on flash the user is about to make the device reboot and flash
/// itself from. A short write here becomes a truncated `.UPG` that the updater runs anyway, and
/// the failure surfaces on the player rather than on the PC where it could still be fixed. The
/// read-back costs a few hundred milliseconds over a whole install and turns that into an error
/// message before anything is triggered.
fn write_verified(path: &Path, bytes: &[u8]) -> io::Result<()> {
    fs::write(path, bytes)?;
    let back = fs::read(path)?;
    if back.len() != bytes.len() || back != bytes {
        return Err(io::Error::other(format!(
            "{} did not read back the same ({} of {} bytes) — the player may be full or the \
             drive may have been disconnected",
            path.display(),
            back.len(),
            bytes.len()
        )));
    }
    Ok(())
}

/// Delete payload files a previous run left in the storage root.
///
/// The device install script prints "left staged binary at /contents/cinder-home (safe to delete
/// once cinder-home is confirmed)" and nothing has ever deleted them, so every install adds
/// another ~3 MB of dead weight to the root of the user's music drive. Never called automatically:
/// the files ARE the fallback if a flash needs repeating.
pub fn clean_leftovers(target: &Path, names: &[String]) -> Vec<(String, io::Result<()>)> {
    names
        .iter()
        .map(|n| (n.clone(), fs::remove_file(target.join(n))))
        .collect()
}

/// Every name an install can leave behind that is safe to delete afterwards.
///
/// `cinder_components.conf` is DELIBERATELY not in this list, even though the installer wrote it
/// and it sits in the same directory. It is the record of which optional parts the user chose, and
/// it is what an update reads back in order to keep those choices. Sweeping it up with the
/// binaries would silently reset the next update to catalogue defaults — turning "free up some
/// space" into "quietly change the user's install".
pub fn payload_names() -> Vec<&'static str> {
    crate::payload::PAYLOAD.iter().map(|(n, _, _)| *n).collect()
}

// ── the handoff, on Windows ────────────────────────────────────────────────────────────────

/// Launch Sony's own Windows updater from an embedded temporary bundle. It owns the device
/// handoff: do not manually eject or trigger a Linux SCSI command before this returns.
///
/// `upg` is the package to hand it, so the same function serves install and uninstall.
#[cfg(windows)]
pub fn run_sony_updater(upg: &[u8]) -> io::Result<()> {
    use std::process::Command;
    let root = std::env::temp_dir().join(format!("cinder-updater-{}", std::process::id()));
    if root.exists() {
        fs::remove_dir_all(&root)?;
    }
    for (relative, bytes) in crate::payload::UPDATER_PAYLOAD {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, bytes)?;
    }
    fs::write(root.join("Data/Device/NW_WM_FW.UPG"), upg)?;
    let exe = root.join("SoftwareUpdateTool.exe");
    let status = Command::new(&exe).current_dir(&root).status()?;
    fs::remove_dir_all(&root).ok();
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("Sony updater exited with {status}")))
    }
}

/// The bytes of the package `action` flashes, for handing to Sony's updater.
#[cfg(windows)]
pub fn package_for(action: Action) -> Option<&'static [u8]> {
    if action.is_removal() {
        crate::payload::UNINSTALL_UPG
    } else {
        crate::payload::PAYLOAD
            .iter()
            .find(|(name, _, _)| *name == UPG_NAME)
            .map(|(_, _, bytes)| *bytes)
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
// WHY NOT SHELL OUT TO scsitool: it is a build-it-yourself binary from a vendored Rockbox
// checkout. The whole point of this installer is that it is one file an end user can run.

/// The vendor CDB. `fc` is Sony's NWZ passthrough opcode; subcommand `04` + the `dbmn` tag is
/// "do firmware upgrade". Byte 8 is a flag: newer devices want 0x80, older ones 0x00, so the
/// caller tries 0x80 first and falls back — the same two-shot `do_fw_upgrade` does.
#[cfg(target_os = "linux")]
pub const FW_UPGRADE_CDB: [u8; 12] = [0xfc, 0, 0x04, b'd', b'b', b'm', b'n', 0, 0, 0, 0, 0];

/// Resolve a mount point to the block device backing it, via /proc/self/mountinfo.
///
/// Field 5 is the mount point and the source follows the " - " separator. Path fields are escaped
/// with octal for space, tab, newline and backslash, so a Walkman mounted at `/media/me/WALKMAN 1`
/// appears as `WALKMAN\0401` and a naive comparison misses it.
#[cfg(target_os = "linux")]
pub fn block_device_for(mount: &Path) -> Option<PathBuf> {
    let text = fs::read_to_string("/proc/self/mountinfo").ok()?;
    block_device_in(&text, mount)
}

/// The parsing half of [`block_device_for`], split out so it can be tested against a captured
/// mountinfo rather than whatever happens to be mounted on the machine running the suite.
#[cfg(target_os = "linux")]
pub fn block_device_in(text: &str, mount: &Path) -> Option<PathBuf> {
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
        return Err(io::Error::other(format!(
            "drive rejected the upgrade command (status {:#x}, host {:#x}, driver {:#x})",
            h.status, h.host_status, h.driver_status
        )));
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
pub fn trigger_fw_upgrade(target: &Path, mut log: impl FnMut(&str)) -> io::Result<()> {
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

    log(&format!("telling the player to reboot into Sony's updater ({})", dev.display()));
    match send_fw_upgrade(&dev, 0x80) {
        Ok(()) => Ok(()),
        Err(first) => {
            log(&format!("newer-style command refused: {first} — trying the older one"));
            send_fw_upgrade(&dev, 0x00)
        }
    }
}

/// macOS has no SG_IO. Raw SCSI passthrough there needs an IOKit `SCSITaskUserClient`, and the
/// kernel will not hand one over for a disk it has already claimed and mounted — which is exactly
/// the state a staged Walkman is in. Rather than pretend, the macOS build stages and says so.
#[cfg(all(unix, not(target_os = "linux")))]
pub fn trigger_fw_upgrade(_target: &Path, _log: impl FnMut(&str)) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "macOS cannot send the vendor SCSI command",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::parse_catalogue;

    const SAMPLE: &str = "\
power | CINDER_POWER | bool | 1 | Power menu
signature | CINDER_SIGNATURE | enum:stock,pv1 | stock | Sound signature
";

    #[test]
    fn an_empty_owner_is_always_staged() {
        let c = parse_catalogue(SAMPLE).unwrap();
        assert!(selected(&c, ""), "the app itself has no owning component");
    }

    #[test]
    fn switching_a_component_off_drops_its_file() {
        let mut c = parse_catalogue(SAMPLE).unwrap();
        assert!(selected(&c, "power"));
        c[0].value = "0".into();
        assert!(!selected(&c, "power"));
    }

    /// An enum component has no "off" — its switcher must be on the device for the choice to be
    /// changeable later, so its files stage at every value including the default.
    #[test]
    fn an_enum_component_stages_at_every_value() {
        let mut c = parse_catalogue(SAMPLE).unwrap();
        for v in ["stock", "pv1"] {
            c[1].value = v.into();
            assert!(selected(&c, "signature"), "signature={v}");
        }
    }

    /// A component the catalogue does not know is staged rather than dropped. The alternative —
    /// silently omitting a file because its owner is unrecognised — produces a partial install
    /// that the device's sanity gate then reverts, with no clue why.
    #[test]
    fn an_unknown_owner_is_staged_not_dropped() {
        let c = parse_catalogue(SAMPLE).unwrap();
        assert!(selected(&c, "no-such-component"));
    }

    /// The uninstall path must write the uninstall package and NOTHING else. Writing the binaries
    /// alongside it would leave the player holding a fresh copy of everything it just removed.
    #[test]
    fn an_uninstall_plan_is_one_file_and_no_conf() {
        let c = parse_catalogue(SAMPLE).unwrap();
        match plan(Action::Uninstall, &c, "stable") {
            Ok(p) => {
                assert_eq!(p.files.len(), 1);
                assert_eq!(p.files[0].0, UPG_NAME);
                assert!(p.conf.is_none(), "an uninstall must not rewrite the component conf");
            }
            // A dist-less checkout has no uninstall package embedded; the error is the contract.
            Err(e) => assert!(e.contains("uninstall package"), "{e}"),
        }
    }

    #[test]
    fn install_and_update_stage_the_same_things() {
        let c = parse_catalogue(SAMPLE).unwrap();
        let (a, b) = (plan(Action::Install, &c, "stable"), plan(Action::Update, &c, "stable"));
        let (a, b) = (a.unwrap(), b.unwrap());
        assert_eq!(a.files.len(), b.files.len());
        assert_eq!(a.conf, b.conf);
    }

    #[test]
    fn only_uninstall_is_a_removal() {
        assert!(Action::Uninstall.is_removal());
        assert!(!Action::Install.is_removal());
        assert!(!Action::Update.is_removal());
    }

    /// A dry run must touch nothing at all. The whole value of the flag is that it can be
    /// pointed at a real, connected Walkman.
    #[test]
    fn a_dry_run_writes_no_files_but_still_reports_the_plan() {
        let c = parse_catalogue(SAMPLE).unwrap();
        let dir = std::env::temp_dir().join(format!("cinder-dry-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let mut seen = 0usize;
        write_payload(Action::Install, &c, "stable", &dir, true, |_, _| seen += 1).unwrap();

        assert!(seen > 0, "a dry run still reports every file it would write");
        let left: Vec<_> = fs::read_dir(&dir).unwrap().flatten().collect();
        assert!(left.is_empty(), "a dry run left {} file(s) behind", left.len());
        let _ = fs::remove_dir_all(&dir);
    }

    /// The component conf is the user's saved answers, and the only copy of them. A cleanup that
    /// removes it turns the next update into a silent reset to defaults.
    #[test]
    fn the_component_conf_is_never_treated_as_a_leftover() {
        assert!(
            !payload_names().contains(&CONF_NAME),
            "cinder_components.conf must survive a cleanup — an update reads it back"
        );
    }

    /// A short write must be caught on the PC, not discovered by the player mid-flash.
    #[test]
    fn a_verified_write_round_trips() {
        let tmp = std::env::temp_dir().join(format!("cinder-stage-{}.bin", std::process::id()));
        write_verified(&tmp, b"hello walkman").unwrap();
        assert_eq!(fs::read(&tmp).unwrap(), b"hello walkman");
        let _ = fs::remove_file(&tmp);
    }

    /// A path whose PARENT IS A FILE, so the failure is deterministic on every platform. The
    /// first version of this used a made-up absolute Unix path, which on Windows resolves against
    /// the current drive and only failed by luck — and CI runs this suite on Windows.
    #[test]
    fn a_write_to_nowhere_is_an_error_not_a_silent_skip() {
        let tmp = std::env::temp_dir().join(format!("cinder-nodir-{}.bin", std::process::id()));
        fs::write(&tmp, b"i am a file, not a directory").unwrap();
        assert!(write_verified(&tmp.join("child.bin"), b"x").is_err());
        let _ = fs::remove_file(&tmp);
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
        assert_eq!(FW_UPGRADE_CDB, [0xfc, 0, 0x04, b'd', b'b', b'm', b'n', 0, 0, 0, 0, 0]);
        let mut with_flag = FW_UPGRADE_CDB;
        with_flag[8] = 0x80;
        assert_eq!(with_flag[8], 0x80, "the flag byte is index 8");
        assert_eq!(&with_flag[..8], &FW_UPGRADE_CDB[..8], "only byte 8 varies");
    }
}
