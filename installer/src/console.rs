//! The text front end.
//!
//! Kept, not replaced. The GUI is the better experience on a desktop, but this is what works over
//! RDP, in a VM, from a rescue shell, on Linux where there is no GUI build, and in a script with
//! `--yes`. It is also the only front end whose output can be pasted into a bug report.

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::catalogue::Comp;
use crate::device::{self, Installed};
use crate::stage::{self, Action};

fn prompt(msg: &str) -> String {
    print!("{msg}");
    let _ = io::stdout().flush();
    let mut s = String::new();
    if io::stdin().read_line(&mut s).is_err() {
        return String::new();
    }
    s.trim().to_string()
}

fn show(comps: &[Comp], target: &Path, action: Action) {
    println!();
    println!("  Cinder {}  (channel: {})", action.verb().to_lowercase(), crate::CHANNEL);
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
    let go = if action == Action::Update { "u update" } else { "i install" };
    println!("\n   <number> toggle/cycle   ?<number> repeat one description   {go}   q quit");
}

/// Pick components, then carry the action out. Returns the process exit code.
pub fn run(mut comps: Vec<Comp>, target: PathBuf, action: Action, assume_yes: bool, dry: bool) -> i32 {
    let state = device::read_installed(&target, &stage::payload_names());
    println!("\n  {}", state.summary());
    if !state.leftovers.is_empty() {
        println!("  ({} staged file(s) from a previous install are still in the root — --clean removes them)",
                 state.leftovers.len());
    }

    if action.is_removal() {
        return uninstall(&target, &state, assume_yes, dry);
    }

    if action == Action::Update {
        match &state.conf {
            Some(text) => {
                let n = crate::catalogue::apply_saved(&mut comps, text);
                println!("  kept {n} component choices from the install already on the player.");
            }
            None => println!("  no saved component choices on the player — using the defaults."),
        }
    }

    if !assume_yes {
        loop {
            show(&comps, &target, action);
            let a = prompt("  > ");
            if a.is_empty() {
                continue;
            }
            match a.as_str() {
                "q" | "Q" => {
                    println!("  nothing written.");
                    return 0;
                }
                "i" | "I" | "u" | "U" => break,
                _ => {}
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

        let plan = match stage::plan(action, &comps, crate::CHANNEL) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("\nERROR: {e}");
                return 1;
            }
        };
        println!("\n  About to write to {}:", target.display());
        for c in &comps {
            println!("    {:<10} {}", c.id, c.value);
        }
        println!("\n  {} files ({} KB) will be copied to the player's storage root.",
                 plan.count(), plan.total_bytes() / 1024);
        println!("  Nothing is flashed by this program — the player does that itself, later.");
        if prompt("\n  Proceed? [y/N] ").to_lowercase() != "y" {
            println!("  nothing written.");
            return 0;
        }
    }

    carry_out(action, &comps, &target, dry)
}

fn uninstall(target: &Path, state: &Installed, assume_yes: bool, dry: bool) -> i32 {
    println!();
    println!("  UNINSTALL removes Cinder and puts the stock Sony player back.");
    println!("  It restores Sony's launch config from the backup the install made and deletes");
    println!("  Cinder's binaries. Your music, playlists and settings on the data partition are");
    println!("  not touched.");
    if !state.present() {
        println!();
        println!("  NOTE: this player does not look like it has Cinder on it ({}).",
                 state.summary().to_lowercase());
        println!("  Running the uninstall anyway is harmless — it is a no-op on a stock device.");
    }
    if !assume_yes && prompt("\n  Uninstall Cinder? [y/N] ").to_lowercase() != "y" {
        println!("  nothing written.");
        return 0;
    }
    carry_out(Action::Uninstall, &[], target, dry)
}

fn carry_out(action: Action, comps: &[Comp], target: &Path, dry: bool) -> i32 {
    if dry {
        println!("\n  DRY RUN — nothing will be written and the player will not be told to flash.");
    }
    println!("\n  {} {} ...", if dry { "would write to" } else { "writing to" }, target.display());
    let r = stage::write_payload(action, comps, crate::CHANNEL, target, dry, |name, n| {
        println!("    {name:<22} {n:>9} bytes");
    });
    if let Err(e) = r {
        eprintln!("\nERROR: {e}");
        eprintln!("The player may now hold a partial copy. Re-run before updating.");
        return 1;
    }

    if dry {
        println!("\n  DRY RUN complete. Nothing was written; the player was not touched.");
        return 0;
    }

    // Every platform that can send the command sends the same one. A failure is reported and
    // then explained rather than exited on: the payload is staged and valid either way, and the
    // user's next move depends on WHY it failed (not elevated, drive held by a file manager,
    // macOS at all).
    {
        println!("\n  Files staged.");
        match stage::trigger_fw_upgrade(target, |m| println!("  {m}")) {
            Ok(()) => print_upgrade_sent(action),
            Err(e) => print_trigger_failed(&e),
        }
        0
    }
}

/// The command went out and the player is rebooting into the updater on its own.
fn print_upgrade_sent(action: Action) {
    let landing = if action.is_removal() { "the stock player" } else { "Cinder" };
    println!("\n  Upgrade command accepted. The player is rebooting into its own updater.");
    println!();
    println!("    * The screen shows the updater, then it reboots into {landing} by itself.");
    println!("    * It DROPS OFF USB while it works. That is expected — leave the cable in and");
    println!("      do not touch it until the player comes back on its own.");
    recovery_note();
}

/// The staging worked and the trigger did not. Which of those two happened decides what the user
/// should do next, so say which, and say why — this is the path that used to print instructions
/// for a menu the NW-A55 does not have.
fn print_trigger_failed(e: &io::Error) {
    println!("\n  Every file is staged on the player, including NW_WM_FW.UPG.");
    println!("  What did NOT happen is the last step: telling it to reboot into its updater.");
    println!("\n    reason: {e}");
    println!();
    if e.kind() == io::ErrorKind::PermissionDenied {
        if cfg!(windows) {
            println!("  That is a permissions error. Sending the command needs raw access to the");
            println!("  player's drive, and Windows grants that only to an elevated process:");
            println!();
            println!("      right-click cinder-installer.exe -> Run as administrator");
            println!();
            println!("  Re-running it is safe — it stages the same files again, then fires.");
        } else {
            println!("  That is a permissions error. Sending a raw SCSI command needs root:");
            println!();
            println!("      sudo {}", std::env::args().next().unwrap_or_else(|| "cinder-installer".into()));
            println!();
            println!("  Re-running it is safe — it stages the same files again, then fires.");
        }
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

fn recovery_note() {
    println!("\n  If a boot ever goes wrong: hold the player's USB cable in at power-on to get");
    println!("  the stock player back, and see RECOVERY.md.");
}

/// Ask which player to use when more than one is plugged in.
pub fn choose_player(found: &[PathBuf]) -> Option<PathBuf> {
    println!("\nSeveral candidates found:");
    for (n, p) in found.iter().enumerate() {
        println!("   {}  {}", n + 1, p.display());
    }
    match prompt("Which one? ").parse::<usize>() {
        Ok(n) if n >= 1 && n <= found.len() => Some(found[n - 1].clone()),
        _ => {
            eprintln!("Not a listed choice — stopping.");
            None
        }
    }
}

/// Delete the payload files a previous install left in the player's storage root.
///
/// Not automatic, and never part of an install. Those files ARE the fallback if a flash has to be
/// repeated, and the device's own installer prints "safe to delete once cinder-home is confirmed"
/// — confirmed means after the player has booted into it, which is a moment only the user is in a
/// position to judge.
pub fn clean(target: &Path) -> i32 {
    let state = device::read_installed(target, &stage::payload_names());
    if state.leftovers.is_empty() {
        println!("\n  Nothing to clean — the storage root has no staged payload files.");
        return 0;
    }
    println!("\n  These staged files are still in {}:", target.display());
    for n in &state.leftovers {
        println!("    {n}");
    }
    println!("\n  They are last install's copies, not the running one. Deleting them frees space");
    println!("  and changes nothing about the Cinder already on the player.");
    if prompt("\n  Delete them? [y/N] ").to_lowercase() != "y" {
        println!("  left alone.");
        return 0;
    }
    let mut bad = 0;
    for (name, r) in stage::clean_leftovers(target, &state.leftovers) {
        match r {
            Ok(()) => println!("    removed {name}"),
            Err(e) => {
                eprintln!("    could not remove {name}: {e}");
                bad += 1;
            }
        }
    }
    i32::from(bad > 0)
}

/// Confirm a target that does not look like a Walkman.
pub fn confirm_odd_target(p: &Path) -> bool {
    eprintln!("WARNING: {} does not look like a Walkman's storage", p.display());
    eprintln!("         (no DevIcon.fil, no MUSIC + PC_Application).");
    prompt("         Use it anyway? [y/N] ").to_lowercase() == "y"
}
