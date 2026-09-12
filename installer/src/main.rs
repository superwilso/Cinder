//! cinder-installer — install, update or remove Cinder on a Sony NW-A50-series Walkman.
//!
//! WHAT IT DOES
//!   1. finds the player's storage (it mounts as a plain USB drive)
//!   2. reads what is already on it, from the device's own install log
//!   3. asks which optional components to install — the catalogue is embedded from
//!      cinder-home/deploy/components.conf, so it can never drift from what the device installer
//!      actually understands
//!   4. writes the answers as cinder_components.conf and copies the staged files to the drive root
//!   5. writes the chosen package as NW_WM_FW.UPG
//!   6. tells the player to reboot into its updater, which applies the package
//!
//! HOW STEP 6 DIFFERS BY PLATFORM. It is one vendor SCSI command either way (see
//! `stage::trigger_fw_upgrade`): Windows sends it through SCSI pass-through on a volume handle,
//! which needs administrator; Linux sends it through `SG_IO`, which needs root. Neither ships any
//! part of Sony's updater any more. On macOS it cannot be sent at all and the installer says so
//! instead of inventing a step. There is NO update entry in the player's own menus on this
//! generation; a host has to trigger it.
//!
//! TWO FRONT ENDS, one core. `gui` is a plain Win32 window (no toolkit, no crates — see its
//! module docs for why); `console` is the text one, and is what runs on Linux, over RDP, and with
//! `--yes`. Both drive `stage`, so there is one implementation of what an install actually is.

mod catalogue;
mod console;
mod device;
mod payload;
mod release;
mod stage;

#[cfg(windows)]
mod gui;

pub use payload::{CATALOGUE, CHANNEL, MISSING};

use std::path::PathBuf;
use stage::Action;

/// This installer's own version, for the stamp it leaves on the player and the release check.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

fn help() {
    println!("cinder-installer {VERSION} — channel {CHANNEL}");
    println!();
    println!("USAGE:  cinder-installer [options] [<drive or mount point>]");
    println!();
    println!("  --install        install Cinder (default when the player has none)");
    println!("  --update         reinstall, keeping the component choices already on the player");
    println!("  --uninstall      put the stock Sony player back");
    println!("  --check          ask GitHub whether a newer release exists, then exit");
    println!("  --clean          delete the staged payload files left in the player's root");
    println!("  --dry-run        walk the whole flow, write nothing, flash nothing");
    println!("  -y, --yes        no questions: take the saved or default answers and go");
    println!("  --console        force the text interface");
    if cfg!(windows) {
        println!("  --gui            force the window (default when double-clicked)");
        println!();
        println!("  To try the window WITHOUT touching a player:  cinder-installer --gui --dry-run");
    }
    println!("  -h, --help       this help");
    println!();
    if cfg!(windows) {
        println!("  The last step is a raw SCSI passthrough — NEEDS ADMINISTRATOR.");
    } else if cfg!(target_os = "linux") {
        println!("  The last step is a raw SCSI passthrough — NEEDS ROOT (run with sudo).");
    } else {
        println!("  This platform can stage files but cannot send the upgrade command;");
        println!("  finish from Linux or Windows. The player has no update menu.");
    }
}

/// Payload problems that make this binary unable to do its job at all.
fn check_payload() -> Result<(), i32> {
    if !MISSING.is_empty() {
        eprintln!("\nERROR: this build is incomplete — the following payload files were missing");
        eprintln!("when it was compiled:");
        for m in MISSING {
            eprintln!("    {m}");
        }
        eprintln!("\nBuild them first:  cinder-home/build.sh {CHANNEL}");
        eprintln!("                   cinder-home/tools/pack_upg.sh {CHANNEL}");
        return Err(2);
    }
    Ok(())
}

fn load_catalogue() -> Result<Vec<catalogue::Comp>, i32> {
    let Some(text) = CATALOGUE else {
        eprintln!("ERROR: no component catalogue embedded (deploy/components.conf missing).");
        return Err(2);
    };
    catalogue::parse_catalogue(text).map_err(|e| {
        eprintln!("ERROR: {e}");
        2
    })
}

/// Ask GitHub, print the answer. `--check` exists so the question can be asked without going
/// anywhere near a plugged-in player.
fn check_for_updates() -> i32 {
    println!("Installed here: {VERSION}");
    println!("Asking {} ...", release::LATEST_URL);
    match release::latest() {
        Ok(r) if release::is_newer(&r.tag, VERSION) => {
            println!("\n  A newer release is out: {} ({})", r.tag, r.name);
            println!("  {}", r.url);
            0
        }
        Ok(r) => {
            println!("\n  Up to date. The latest release is {}.", r.tag);
            0
        }
        Err(e) => {
            eprintln!("\n  Could not check: {e}");
            eprintln!("  This does not affect installing — the payload is embedded in this file.");
            1
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut explicit: Option<PathBuf> = None;
    let mut assume_yes = false;
    let mut action: Option<Action> = None;
    let mut force_console = false;
    let mut force_gui = false;
    let mut clean = false;
    let mut dry = false;

    for a in &args {
        match a.as_str() {
            "-y" | "--yes" => assume_yes = true,
            "--console" | "--text" => force_console = true,
            "--clean" => clean = true,
            "--dry-run" | "--dry" => dry = true,
            "--gui" => force_gui = true,
            "--install" => action = Some(Action::Install),
            "--update" => action = Some(Action::Update),
            "--uninstall" | "--remove" => action = Some(Action::Uninstall),
            "--check" | "--check-updates" => std::process::exit(check_for_updates()),
            "-h" | "--help" => {
                help();
                return;
            }
            "-V" | "--version" => {
                println!("cinder-installer {VERSION} (channel {CHANNEL})");
                return;
            }
            other if other.starts_with('-') => {
                eprintln!("unknown option: {other}");
                help();
                std::process::exit(2);
            }
            other => explicit = Some(PathBuf::from(other)),
        }
    }

    // The window is the right default for someone who downloaded an .exe and double-clicked it,
    // and the wrong one for every scripted or remote use. `owns_its_console` is what tells those
    // apart: a double-click gets a console created for it and nothing else in it.
    #[cfg(windows)]
    let want_gui =
        force_gui || (!force_console && !assume_yes && !clean && action.is_none() && gui::owns_its_console());
    // Both only steer the Windows front-end choice; off Windows there is one front end.
    #[cfg(not(windows))]
    let _ = (force_console, force_gui);

    #[cfg(windows)]
    if want_gui {
        std::process::exit(gui::run(action, dry));
    }

    println!("Cinder installer {VERSION} — channel {CHANNEL}");
    if let Err(rc) = check_payload() {
        std::process::exit(rc);
    }
    let comps = match load_catalogue() {
        Ok(c) => c,
        Err(rc) => std::process::exit(rc),
    };

    // ── locate the player ──
    let target = match explicit {
        Some(p) => {
            if !p.is_dir() {
                eprintln!("ERROR: {} is not a directory.", p.display());
                std::process::exit(1);
            }
            if !device::looks_like_walkman(&p) && !assume_yes && !console::confirm_odd_target(&p) {
                return;
            }
            p
        }
        None => {
            let found = device::find_players();
            match found.len() {
                0 => {
                    eprintln!("\nERROR: no Walkman found.");
                    eprintln!("  Connect the player by USB and set it to mass-storage mode, then");
                    eprintln!("  run this again. If it is mounted somewhere unusual, pass the path:");
                    if cfg!(windows) {
                        eprintln!("      cinder-installer D:\\");
                    } else {
                        eprintln!("      cinder-installer /media/you/WALKMAN");
                    }
                    std::process::exit(1);
                }
                1 => found[0].clone(),
                _ => match console::choose_player(&found) {
                    Some(p) => p,
                    None => std::process::exit(1),
                },
            }
        }
    };

    if clean {
        std::process::exit(console::clean(&target));
    }

    // With no explicit action, do what the player's state implies: update what is there, install
    // what is not. Removal is never implied — it is always asked for by name.
    let action = action.unwrap_or_else(|| {
        if device::read_installed(&target, &stage::payload_names()).present() {
            Action::Update
        } else {
            Action::Install
        }
    });

    std::process::exit(console::run(comps, target, action, assume_yes, dry));
}
