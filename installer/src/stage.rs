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
    // `a|b`: a file two components share, staged when either is on.
    owner.split('|').any(|o| {
        comps
            .iter()
            .find(|c| c.id == o)
            .map(Comp::is_on)
            .unwrap_or(true)
    })
}

/// Is this payload file staged? A `fw:<model>` owner is the install package sealed for that
/// model's key: the Walkman One one (`nw-wm1a`) on a player that runs it, stock's otherwise.
fn staged(comps: &[Comp], owner: &str, w1: bool) -> bool {
    match owner.strip_prefix("fw:") {
        Some(model) => (model == "nw-wm1a") == w1,
        None => selected(comps, owner),
    }
}

/// The uninstall package for this player's key.
fn uninstall_upg(w1: bool) -> Result<&'static [u8], String> {
    let (bytes, name) = if w1 {
        (
            crate::payload::UNINSTALL_UPG_W1,
            "cinder_home_uninstall.nw-wm1a.upg",
        )
    } else {
        (crate::payload::UNINSTALL_UPG, "cinder_home_uninstall.upg")
    };
    bytes.ok_or_else(|| format!("this build has no uninstall package embedded ({name} was missing when it was compiled)"))
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

/// Build the plan for `action` on a player that runs Walkman One (`w1`) or not. Returns Err when
/// the build cannot carry it out at all: no package embedded for this player's key.
pub fn plan(action: Action, comps: &[Comp], channel: &str, w1: bool) -> Result<Plan, String> {
    if action.is_removal() {
        let bytes = uninstall_upg(w1)?;
        return Ok(Plan {
            files: vec![(UPG_NAME.to_string(), bytes.len())],
            conf: None,
        });
    }

    let files: Vec<(String, usize)> = crate::payload::PAYLOAD
        .iter()
        .filter(|(_, owner, _)| staged(comps, owner, w1))
        .map(|(name, _, bytes)| ((*name).to_string(), bytes.len()))
        .collect();
    if !files.iter().any(|(n, _)| n == UPG_NAME) {
        return Err(format!(
            "this build has no install package for {} players",
            if w1 { "Walkman One" } else { "stock-firmware" }
        ));
    }
    Ok(Plan {
        files,
        conf: Some(crate::catalogue::conf_text(comps, channel)),
    })
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
    // The same reading the confirm screen's plan was built from: the drive itself.
    let w1 = crate::device::running_walkman_one(target);
    plan(action, comps, channel, w1).map_err(|e| io::Error::new(io::ErrorKind::NotFound, e))?;
    if action.is_removal() {
        let bytes = uninstall_upg(w1).map_err(|e| io::Error::new(io::ErrorKind::NotFound, e))?;
        if !dry {
            write_verified(&target.join(UPG_NAME), bytes)?;
        }
        progress(UPG_NAME, bytes.len());
        return Ok(());
    }

    // Sony's volume tables, when the chosen curve needs them and the drive does not already hold
    // them (sony_tables.rs). Fetched before anything is written, so a failed download leaves the
    // drive as it was. A dry run does not go online.
    use crate::sony_tables as st;
    let curve = comps
        .iter()
        .find(|c| c.id == "voltable")
        .map_or("", |c| c.value.as_str());
    let fetch = st::needed(curve) && !st::TABLES.iter().all(|t| st::on_drive(target, t.0));
    let tables: Vec<(&str, Vec<u8>)> = if fetch && !dry {
        st::fetch().map_err(|e| {
            io::Error::other(format!(
                "could not get Sony's volume tables for the chosen curve from {}: {e}. Check the \
                 connection, or choose \"Sony standard\" and install again",
                st::URL.split('/').nth(2).unwrap_or(st::URL)
            ))
        })?
    } else {
        Vec::new()
    };
    // The low-latency USB DAC module, the same way (llusbdac.rs).
    use crate::llusbdac as ll;
    let ll_fetch = ll::needed(comps) && !ll::on_drive(target);
    let module = if ll_fetch && !dry {
        Some(ll::fetch().map_err(|e| {
            io::Error::other(format!(
                "could not get the low-latency USB DAC module from GitHub: {e}. Check the \
                 connection, or untick \"Low-latency USB DAC\" and install again"
            ))
        })?)
    } else {
        None
    };

    for (name, owner, bytes) in crate::payload::PAYLOAD {
        if !staged(comps, owner, w1) {
            continue;
        }
        if !dry {
            write_verified(&target.join(name), bytes)?;
        }
        progress(name, bytes.len());
    }
    if fetch && dry {
        for (name, _, size, _) in st::TABLES {
            progress(name, size);
        }
    }
    for (name, bytes) in &tables {
        write_verified(&target.join(name), bytes)?;
        progress(name, bytes.len());
    }
    if let Some(bytes) = &module {
        write_verified(&target.join(ll::NAME), bytes)?;
        progress(ll::NAME, bytes.len());
    } else if ll_fetch && dry {
        progress(ll::NAME, 0);
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
///
/// `cinder_advanced.conf.example` is left out too: it is the reference Help points to, not a
/// staged binary.
pub fn payload_names() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = crate::payload::PAYLOAD
        .iter()
        .map(|(n, _, _)| *n)
        .filter(|n| *n != ADVANCED_EXAMPLE)
        .collect();
    for r in RETIRED {
        if !names.contains(r) {
            names.push(r);
        }
    }
    names
}

/// Names earlier installers staged and this one does not: still dead weight on the user's drive.
/// cinder-probe left the stable payload, and cinder-gpunode the project, on 2026-10-05.
const ADVANCED_EXAMPLE: &str = "cinder_advanced.conf.example";

const RETIRED: &[&str] = &["cinder-probe", "cinder-gpunode"];

// ── the firmware-upgrade trigger, on Windows ───────────────────────────────────────────────
//
// WHAT THIS REPLACED. Until 0.3.1 this section launched Sony's own `SoftwareUpdateTool.exe` from
// a temporary directory, and every Windows installer carried Sony's updater — the .exe, Sony's
// `WmFwUpdater.dll` and Microsoft's Visual C++ 2010 runtime — embedded in it. That is Sony's
// software shipped inside a Cinder download, and it was the largest piece of someone else's
// property this project distributed.
//
// It was never needed for its own sake. Sony's tool ends in exactly one thing this installer
// already does on Linux: the 12-byte vendor SCSI command below, which tells the player to reboot
// into its updater and apply the `NW_WM_FW.UPG` sitting on its data partition. `write_payload`
// stages that file on every platform, so the tool was copying a file that was already there and
// then sending a command Windows can send itself.
//
// WHY IT NEEDS ADMINISTRATOR. The command goes to a *volume* handle (`\\.\E:`) opened for read
// and write, and Windows gives raw volume access only to an elevated process. That is why
// `cinder-installer.manifest` asks for `requireAdministrator` — the one thing in this installer
// that needs a right beyond writing files to a removable drive.

/// The vendor CDB. `fc` is Sony's NWZ passthrough opcode; subcommand `04` + the `dbmn` tag is
/// "do firmware upgrade". Byte 8 is a flag: newer devices want 0x80, older ones 0x00, so the
/// caller tries 0x80 first and falls back — the same two-shot `do_fw_upgrade` does.
///
/// The same twelve bytes go out on Linux through `SG_IO` and on Windows through SCSI
/// pass-through. Keeping one constant is deliberate: two copies of a device command drift.
#[cfg(any(target_os = "linux", windows, target_os = "macos", test))]
pub const FW_UPGRADE_CDB: [u8; 12] = [0xfc, 0, 0x04, b'd', b'b', b'm', b'n', 0, 0, 0, 0, 0];

/// Read the player's KAS, the key Sony's updater decrypts a package with: DNK property 0x23,
/// entry 22, 64 bytes of hex text (Rockbox `scsitool get_dnk_prop kas`). Walkman One rewrites it,
/// so it says which package this player takes. Bytes 8-9 are the length, 7 must be 0xbc.
#[cfg(any(target_os = "linux", windows, test))]
pub const KAS_CDB: [u8; 12] = [0xdd, 0, 0, 0, 0, 0, 0, 0xbc, 0, 64, 0x23, 22];

/// The KAS an NW-A50-series player reports under Walkman One: the NW-WM1A's (`upgtool -m ?`).
pub const W1_KAS: &str = "e8d171a5d92f35eed9658c03fb9f86a169591659851fd7c49525f587a70b526c";

/// The key text out of a KAS reply: printable bytes up to the first NUL, lower case.
#[cfg(any(target_os = "linux", windows, test))]
pub fn kas_text(reply: &[u8]) -> String {
    reply
        .iter()
        .take_while(|&&b| b != 0)
        .filter(|b| b.is_ascii_graphic())
        .map(|b| b.to_ascii_lowercase() as char)
        .collect()
}

#[cfg(windows)]
mod scsi {
    use std::os::raw::c_void;

    pub type Handle = *mut c_void;

    pub const GENERIC_READ: u32 = 0x8000_0000;
    pub const GENERIC_WRITE: u32 = 0x4000_0000;
    pub const FILE_SHARE_READ: u32 = 0x0000_0001;
    pub const FILE_SHARE_WRITE: u32 = 0x0000_0002;
    pub const OPEN_EXISTING: u32 = 3;

    /// `CTL_CODE(IOCTL_SCSI_BASE, 0x0405, METHOD_BUFFERED, FILE_READ_ACCESS|FILE_WRITE_ACCESS)`,
    /// i.e. `(4 << 16) | (3 << 14) | (0x405 << 2)`. Spelled out because the arithmetic is the
    /// only thing that says this value is not a magic number.
    pub const IOCTL_SCSI_PASS_THROUGH_DIRECT: u32 = (4 << 16) | (3 << 14) | (0x405 << 2);
    pub const FSCTL_LOCK_VOLUME: u32 = 0x0009_0018;
    pub const FSCTL_DISMOUNT_VOLUME: u32 = 0x0009_0020;

    /// `SCSI_IOCTL_DATA_IN` — the device sends, we receive. Matches `SG_DXFER_FROM_DEV` on Linux.
    pub const SCSI_IOCTL_DATA_IN: u8 = 1;
    pub const ERROR_ACCESS_DENIED: i32 = 5;

    /// `SCSI_PASS_THROUGH_DIRECT` from `ntddscsi.h`. `#[repr(C)]` reproduces the padding the
    /// header gets from the compiler, and `Length` is filled from `size_of` rather than a
    /// literal, so the 32-bit and 64-bit builds each declare their own size.
    #[repr(C)]
    pub struct PassThroughDirect {
        pub length: u16,
        pub scsi_status: u8,
        pub path_id: u8,
        pub target_id: u8,
        pub lun: u8,
        pub cdb_length: u8,
        pub sense_info_length: u8,
        pub data_in: u8,
        pub data_transfer_length: u32,
        pub timeout_value: u32,
        pub data_buffer: *mut c_void,
        pub sense_info_offset: u32,
        pub cdb: [u8; 16],
    }

    /// The struct and its sense buffer in one allocation, which is what `SenseInfoOffset` is for:
    /// the port driver writes the sense data at that offset from the start of the request.
    #[repr(C)]
    pub struct WithSense {
        pub spt: PassThroughDirect,
        pub sense: [u8; 32],
    }

    /// The data buffer a DIRECT request hands to the port driver, which maps it and therefore
    /// requires it to satisfy the adapter's alignment mask. USB mass storage asks for 0 or 3;
    /// a page is a safe superset and costs one page.
    #[repr(C, align(4096))]
    pub struct AlignedBuf(pub [u8; 128]);

    #[link(name = "kernel32")]
    extern "system" {
        pub fn CreateFileW(
            name: *const u16,
            access: u32,
            share: u32,
            security: *mut c_void,
            disposition: u32,
            flags: u32,
            template: Handle,
        ) -> Handle;
        pub fn DeviceIoControl(
            device: Handle,
            code: u32,
            in_buf: *mut c_void,
            in_len: u32,
            out_buf: *mut c_void,
            out_len: u32,
            returned: *mut u32,
            overlapped: *mut c_void,
        ) -> i32;
        pub fn FlushFileBuffers(h: Handle) -> i32;
        pub fn CloseHandle(h: Handle) -> i32;
    }
}

/// `E:\` → `\\.\E:`, as a NUL-terminated UTF-16 string.
///
/// The installer's target is a mount root chosen from the drive list, so it always starts with a
/// letter and a colon. Anything else — a UNC share, a mounted folder — has no volume device path
/// of this shape, and guessing one would send a device command to whatever it did resolve to.
#[cfg(windows)]
fn volume_device_path(target: &Path) -> io::Result<Vec<u16>> {
    let text = target.to_string_lossy();
    let mut chars = text.chars();
    let letter = chars.next().unwrap_or(' ');
    if !letter.is_ascii_alphabetic() || chars.next() != Some(':') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "{} is not a drive letter, so it has no volume to send the command to",
                text
            ),
        ));
    }
    let path = format!(r"\\.\{}:", letter.to_ascii_uppercase());
    Ok(path.encode_utf16().chain(std::iter::once(0)).collect())
}

/// Fire the vendor command at an open volume handle with one flag byte. Ok(()) means the drive
/// accepted it — the same contract as the Linux `send_fw_upgrade`.
#[cfg(windows)]
fn send_fw_upgrade(vol: scsi::Handle, flag: u8) -> io::Result<()> {
    let mut cdb = FW_UPGRADE_CDB;
    cdb[8] = flag;
    send_cdb(vol, &cdb).map(|_| ())
}

/// Send one 12-byte vendor CDB that reads from the drive, and return what it sent back.
#[cfg(windows)]
fn send_cdb(vol: scsi::Handle, cdb12: &[u8; 12]) -> io::Result<[u8; 128]> {
    let mut buf = scsi::AlignedBuf([0u8; 128]);
    let mut cdb = [0u8; 16];
    cdb[..cdb12.len()].copy_from_slice(cdb12);

    let mut req = scsi::WithSense {
        spt: scsi::PassThroughDirect {
            length: std::mem::size_of::<scsi::PassThroughDirect>() as u16,
            scsi_status: 0,
            path_id: 0,
            target_id: 0,
            lun: 0,
            cdb_length: cdb12.len() as u8,
            sense_info_length: 32,
            data_in: scsi::SCSI_IOCTL_DATA_IN,
            data_transfer_length: buf.0.len() as u32,
            timeout_value: 30,
            data_buffer: buf.0.as_mut_ptr().cast(),
            sense_info_offset: std::mem::size_of::<scsi::PassThroughDirect>() as u32,
            cdb,
        },
        sense: [0u8; 32],
    };

    let len = std::mem::size_of::<scsi::WithSense>() as u32;
    let mut returned = 0u32;
    // SAFETY: req outlives the call; data_buffer addresses buf, which is alive and at least
    // data_transfer_length bytes; the request is passed as both input and output buffer, which is
    // what a METHOD_BUFFERED pass-through expects.
    let ok = unsafe {
        scsi::DeviceIoControl(
            vol,
            scsi::IOCTL_SCSI_PASS_THROUGH_DIRECT,
            (&mut req as *mut scsi::WithSense).cast(),
            len,
            (&mut req as *mut scsi::WithSense).cast(),
            len,
            &mut returned,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    // A non-zero SCSI status means the drive understood the transport and rejected the command —
    // the signal to try the other flag byte, not to fail the install. The sense key is the top
    // nibble of byte 2 of fixed-format sense data, and it is worth reporting: 0x5 is "illegal
    // request", which is what the wrong flag byte looks like.
    if req.spt.scsi_status != 0 {
        return Err(io::Error::other(format!(
            "drive rejected the command (status {:#x}, sense key {:#x})",
            req.spt.scsi_status,
            req.sense[2] & 0x0f
        )));
    }
    Ok(buf.0)
}

/// The player's KAS ([`KAS_CDB`]), read through the volume behind `target`. Needs administrator,
/// as the upgrade command does.
#[cfg(windows)]
pub fn read_kas(target: &Path) -> io::Result<String> {
    let name = volume_device_path(target)?;
    // SAFETY: name is NUL-terminated and lives across the call.
    let vol = unsafe {
        scsi::CreateFileW(
            name.as_ptr(),
            scsi::GENERIC_READ | scsi::GENERIC_WRITE,
            scsi::FILE_SHARE_READ | scsi::FILE_SHARE_WRITE,
            std::ptr::null_mut(),
            scsi::OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    };
    if vol == usize::MAX as scsi::Handle {
        return Err(io::Error::last_os_error());
    }
    let r = send_cdb(vol, &KAS_CDB);
    // SAFETY: vol was opened above and is closed exactly once.
    unsafe { scsi::CloseHandle(vol) };
    Ok(kas_text(&r?[..64]))
}

/// Flush, dismount, then tell the player to reboot into its updater.
///
/// ORDER MATTERS, for the same reason it does on Linux: the payload has just been written through
/// the cache, and a device that reboots before those bytes reach flash finds a truncated
/// `NW_WM_FW.UPG`. `FlushFileBuffers` on the volume handle is the write-back; the lock and
/// dismount are best-effort, because Explorer may hold the volume and a flushed-but-mounted
/// player still updates correctly, while one that never got the bytes does not.
#[cfg(windows)]
pub fn trigger_fw_upgrade(target: &Path, mut log: impl FnMut(&str)) -> io::Result<()> {
    let name = volume_device_path(target)?;
    let invalid = usize::MAX as scsi::Handle;

    // SAFETY: name is NUL-terminated and lives across the call.
    let vol = unsafe {
        scsi::CreateFileW(
            name.as_ptr(),
            scsi::GENERIC_READ | scsi::GENERIC_WRITE,
            scsi::FILE_SHARE_READ | scsi::FILE_SHARE_WRITE,
            std::ptr::null_mut(),
            scsi::OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    };
    if vol == invalid {
        let e = io::Error::last_os_error();
        return Err(if e.raw_os_error() == Some(scsi::ERROR_ACCESS_DENIED) {
            io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Windows refused raw access to the player's drive. \
                 Run the installer as administrator.",
            )
        } else {
            e
        });
    }

    struct Owned(scsi::Handle);
    impl Drop for Owned {
        fn drop(&mut self) {
            // SAFETY: self.0 came from a successful CreateFileW and is closed exactly once.
            unsafe { scsi::CloseHandle(self.0) };
        }
    }
    let vol = Owned(vol);

    // SAFETY: vol.0 is a live volume handle for every call below; each in/out buffer pointer is
    // null with a zero length, which is what these two FSCTLs take.
    unsafe {
        scsi::FlushFileBuffers(vol.0);
        let mut returned = 0u32;
        for code in [scsi::FSCTL_LOCK_VOLUME, scsi::FSCTL_DISMOUNT_VOLUME] {
            scsi::DeviceIoControl(
                vol.0,
                code,
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                0,
                &mut returned,
                std::ptr::null_mut(),
            );
        }
    }

    log("telling the player to reboot into its updater");
    match send_fw_upgrade(vol.0, 0x80) {
        Ok(()) => Ok(()),
        Err(first) => {
            log(&format!(
                "newer-style command refused: {first} — trying the older one"
            ));
            send_fw_upgrade(vol.0, 0x00)
        }
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
    let mut cdb = FW_UPGRADE_CDB;
    cdb[8] = flag;
    send_cdb(dev, &cdb).map(|_| ())
}

/// The player's KAS ([`KAS_CDB`]), read from the block device behind `target`. Needs root, as the
/// upgrade command does.
#[cfg(target_os = "linux")]
pub fn read_kas(target: &Path) -> io::Result<String> {
    let dev = block_device_for(target)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no block device"))?;
    Ok(kas_text(&send_cdb(&dev, &KAS_CDB)?[..64]))
}

/// Send one 12-byte vendor CDB that reads from `dev`, and return what it sent back.
#[cfg(target_os = "linux")]
fn send_cdb(dev: &Path, cdb: &[u8; 12]) -> io::Result<[u8; 0x80]> {
    use std::os::unix::io::AsRawFd;

    let f = fs::OpenOptions::new().read(true).write(true).open(dev)?;
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
    let rc = unsafe {
        sg::ioctl(
            f.as_raw_fd(),
            sg::SG_IO,
            (&mut h as *mut sg::SgIoHdr).cast(),
        )
    };
    if rc < 0 {
        return Err(io::Error::last_os_error());
    }
    // A non-zero SCSI status means the drive understood the transport and rejected the command —
    // that is the signal to try the other flag byte, not to fail the install.
    if h.status != 0 || h.host_status != 0 || h.driver_status != 0 {
        return Err(io::Error::other(format!(
            "drive rejected the command (status {:#x}, host {:#x}, driver {:#x})",
            h.status, h.host_status, h.driver_status
        )));
    }
    Ok(buf)
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
            format!(
                "could not find the block device behind {}",
                target.display()
            ),
        )
    })?;

    // SAFETY: sync() takes no arguments and cannot fail.
    unsafe { sg::sync() };

    let mut c: Vec<u8> = target.as_os_str().as_encoded_bytes().to_vec();
    c.push(0);
    // SAFETY: c is NUL-terminated and lives across the call.
    unsafe { sg::umount(c.as_ptr()) };

    log(&format!(
        "telling the player to reboot into its own updater ({})",
        dev.display()
    ));
    match send_fw_upgrade(&dev, 0x80) {
        Ok(()) => Ok(()),
        Err(first) => {
            log(&format!(
                "newer-style command refused: {first} — trying the older one"
            ));
            send_fw_upgrade(&dev, 0x00)
        }
    }
}

/// The key is not read on macOS: taking the player's USB interface (below) ejects its volume, and
/// the key is wanted before the files are staged onto it.
#[cfg(all(unix, not(target_os = "linux")))]
pub fn read_kas(_target: &Path) -> io::Result<String> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "the key is not read on this system",
    ))
}

/// Other Unixes have no way in at all; the staging is done and they say so.
#[cfg(all(unix, not(any(target_os = "linux", target_os = "macos"))))]
pub fn trigger_fw_upgrade(_target: &Path, _log: impl FnMut(&str)) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "this system cannot send the vendor SCSI command",
    ))
}

/// Sony's USB vendor id.
#[cfg(target_os = "macos")]
const SONY_VID: u16 = 0x054c;

#[cfg(target_os = "macos")]
fn usb_err(e: rusb::Error) -> io::Error {
    let kind = match e {
        rusb::Error::Access => io::ErrorKind::PermissionDenied,
        rusb::Error::NotFound | rusb::Error::NoDevice => io::ErrorKind::NotFound,
        rusb::Error::Timeout => io::ErrorKind::TimedOut,
        _ => io::ErrorKind::Other,
    };
    io::Error::new(kind, format!("USB: {e}"))
}

/// The player's mass-storage interface, taken from macOS: (handle, bulk-OUT, bulk-IN). macOS
/// SCSI passthrough refuses a disk it has mounted, so the command goes out as raw USB mass storage
/// (`bot.rs`) instead. Taking the interface from macOS's driver needs root and libusb's "capture"
/// (libusb 1.0.25+, which rusb's vendored build is).
#[cfg(target_os = "macos")]
fn open_player() -> io::Result<(rusb::DeviceHandle<rusb::Context>, u8, u8)> {
    use rusb::UsbContext;
    let ctx = rusb::Context::new().map_err(usb_err)?;
    let mut found = Vec::new();
    for dev in ctx.devices().map_err(usb_err)?.iter() {
        let Ok(d) = dev.device_descriptor() else {
            continue;
        };
        if d.vendor_id() != SONY_VID {
            continue;
        }
        let Ok(cfg) = dev.active_config_descriptor() else {
            continue;
        };
        let mut msc = None;
        for iface in cfg.interfaces() {
            for alt in iface.descriptors() {
                // Mass storage, SCSI transparent command set, bulk-only transport.
                if (alt.class_code(), alt.sub_class_code(), alt.protocol_code()) != (8, 6, 0x50) {
                    continue;
                }
                let (mut out, mut inp) = (None, None);
                for ep in alt.endpoint_descriptors() {
                    if ep.transfer_type() == rusb::TransferType::Bulk {
                        match ep.direction() {
                            rusb::Direction::Out => out = Some(ep.address()),
                            rusb::Direction::In => inp = Some(ep.address()),
                        }
                    }
                }
                if let (Some(o), Some(i)) = (out, inp) {
                    msc = Some((alt.interface_number(), o, i));
                }
            }
        }
        if let Some(m) = msc {
            found.push((dev, m));
        }
    }
    if found.len() > 1 {
        return Err(io::Error::other(format!(
            "{} Sony storage devices are plugged in; unplug all but the player",
            found.len()
        )));
    }
    let Some((dev, (n, out, inp))) = found.pop() else {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "no Sony storage device on USB",
        ));
    };
    let h = dev.open().map_err(usb_err)?;
    if h.kernel_driver_active(n).unwrap_or(true) {
        h.detach_kernel_driver(n).map_err(usb_err)?;
    }
    h.claim_interface(n).map_err(usb_err)?;
    Ok((h, out, inp))
}

/// Send one 12-byte CDB that reads up to `want` bytes, over raw USB mass storage, and return what
/// came back. `reboots`: the command restarts the player, so the player vanishing before its
/// status arrives is success, not failure.
#[cfg(target_os = "macos")]
fn send_cdb_usb(cdb: &[u8; 12], want: usize, reboots: bool) -> io::Result<Vec<u8>> {
    use crate::bot;
    let (h, out, inp) = open_player()?;
    let t = std::time::Duration::from_secs(30);
    let tag = u32::from_le_bytes(*b"Cind");
    h.write_bulk(out, &bot::cbw(tag, want as u32, cdb), t)
        .map_err(usb_err)?;
    let gone = |e: &rusb::Error| reboots && matches!(e, rusb::Error::NoDevice | rusb::Error::Io);
    let mut data = vec![0u8; want];
    // A device may send less than asked or stall the data stage; the status follows either way.
    let got = match h.read_bulk(inp, &mut data, t) {
        Ok(n) => n,
        Err(rusb::Error::Pipe) => {
            let _ = h.clear_halt(inp);
            0
        }
        Err(e) if gone(&e) => return Ok(Vec::new()),
        Err(e) => return Err(usb_err(e)),
    };
    data.truncate(got);
    let mut st = [0u8; 13];
    let n = match h.read_bulk(inp, &mut st, t) {
        Ok(n) => n,
        Err(e) if gone(&e) => return Ok(data),
        Err(e) => return Err(usb_err(e)),
    };
    match bot::csw(&st[..n], tag).map_err(io::Error::other)? {
        bot::Status::Passed => Ok(data),
        s => Err(io::Error::other(format!(
            "drive rejected the command ({s:?})"
        ))),
    }
}

/// Flush, unmount the player's volume (the disk stays attached), then tell it to reboot into its
/// updater over raw USB mass storage. The order matters for the reason the Linux one gives.
#[cfg(target_os = "macos")]
pub fn trigger_fw_upgrade(target: &Path, mut log: impl FnMut(&str)) -> io::Result<()> {
    use std::process::Command;
    let _ = Command::new("sync").status();
    let unmounted = Command::new("diskutil")
        .arg("unmountDisk")
        .arg(target)
        .status()
        .is_ok_and(|s| s.success());
    if !unmounted {
        log("diskutil could not unmount the player; trying anyway");
    }
    log("telling the player to reboot into its own updater (raw USB)");
    let fire = |flag: u8| {
        let mut cdb = FW_UPGRADE_CDB;
        cdb[8] = flag;
        send_cdb_usb(&cdb, 0x80, true).map(|_| ())
    };
    match fire(0x80) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => Err(e),
        Err(first) => {
            log(&format!(
                "newer-style command refused: {first} — trying the older one"
            ));
            fire(0x00)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::parse_catalogue;

    const SAMPLE: &str = "\
power | CINDER_POWER | bool | 1 | Power menu
signature | CINDER_SIGNATURE | enum:stock,pv1 | stock | Sound signature
    = stock | Sony standard | No change.
    = pv1   | Plus v1       | Walkman One's first.
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

    /// The mono shim belongs to `mono` and `preload`. The installer offers `preload` instead of
    /// `mono` on a player without Wampy, and with only `preload` ticked the shim was not staged, so
    /// preload and everything after it (AAC, FM recording) were silently not installed (10-28).
    #[test]
    fn a_shared_file_stages_when_either_owner_is_on() {
        let mut c = parse_catalogue(
            "mono | CINDER_MONO | bool | 1 | Mono\npreload | CINDER_PRELOAD | bool | 0 | Preload\n",
        )
        .unwrap();
        c[0].value = "0".into();
        assert!(!selected(&c, "mono|preload"));
        c[1].value = "1".into();
        assert!(selected(&c, "mono|preload"), "preload alone needs the shim");
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
        match plan(Action::Uninstall, &c, "stable", false) {
            Ok(p) => {
                assert_eq!(p.files.len(), 1);
                assert_eq!(p.files[0].0, UPG_NAME);
                assert!(
                    p.conf.is_none(),
                    "an uninstall must not rewrite the component conf"
                );
            }
            // A dist-less checkout has no uninstall package embedded; the error is the contract.
            Err(e) => assert!(e.contains("uninstall package"), "{e}"),
        }
    }

    #[test]
    fn install_and_update_stage_the_same_things() {
        let c = parse_catalogue(SAMPLE).unwrap();
        let (a, b) = (
            plan(Action::Install, &c, "stable", false),
            plan(Action::Update, &c, "stable", false),
        );
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

        assert!(
            seen > 0,
            "a dry run still reports every file it would write"
        );
        let left: Vec<_> = fs::read_dir(&dir).unwrap().flatten().collect();
        assert!(
            left.is_empty(),
            "a dry run left {} file(s) behind",
            left.len()
        );
        let _ = fs::remove_dir_all(&dir);
    }

    /// The component conf is the user's saved answers, and the only copy of them. A cleanup that
    /// removes it turns the next update into a silent reset to defaults.
    /// An old install's probe is swept up like any other leftover, and listed once on dev too.
    #[test]
    fn a_retired_file_is_still_a_leftover() {
        let names = payload_names();
        assert_eq!(names.iter().filter(|n| **n == "cinder-probe").count(), 1);
    }

    #[test]
    fn the_component_conf_is_never_treated_as_a_leftover() {
        assert!(
            !payload_names().contains(&CONF_NAME),
            "cinder_components.conf must survive a cleanup — an update reads it back"
        );
    }

    /// It is staged on every install, and kept: Help points at it.
    #[test]
    fn the_advanced_example_is_staged_and_never_a_leftover() {
        assert!(crate::payload::PAYLOAD
            .iter()
            .any(|(n, owner, _)| *n == ADVANCED_EXAMPLE && owner.is_empty()));
        assert!(!payload_names().contains(&ADVANCED_EXAMPLE));
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
        assert_eq!(
            block_device_in(MOUNTINFO, Path::new("/run/user/1000/doc")),
            None
        );
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn unknown_mount_point_is_not_a_guess() {
        assert_eq!(
            block_device_in(MOUNTINFO, Path::new("/media/me/NOPE")),
            None
        );
    }

    #[test]
    fn the_package_follows_the_key_and_the_kas_reply_is_read_as_text() {
        assert!(staged(&[], "fw:nw-a50", false) && !staged(&[], "fw:nw-a50", true));
        assert!(staged(&[], "fw:nw-wm1a", true) && !staged(&[], "fw:nw-wm1a", false));
        let mut reply = [0u8; 64];
        reply.copy_from_slice(W1_KAS.to_ascii_uppercase().as_bytes());
        assert_eq!(kas_text(&reply), W1_KAS);
        assert_eq!(kas_text(&[0u8; 64]), "");
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
}
