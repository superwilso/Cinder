//! One file a user can attach to an issue: everything this installer can see on the player.
//!
//! Every "the install did nothing" report has needed the same files off the drive, and asking for
//! them one at a time cost days of an issue thread (#41). This gathers them in one click. It reads
//! the drive and writes only the report, on the computer.

use crate::{device, stage};
use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Files at the top of the drive, and how many of their last lines to keep (0 = all of it).
const FILES: &[(&str, usize)] = &[
    (device::LOG_NAME, 400),
    // The package's first step writes it before running the install script; no file means
    // Sony's updater stopped before Cinder's code ran at all.
    ("exec.txt", 0),
    (device::CONF_NAME, 0),
    (device::HOME_LOG, 300),
    ("CFW/boot_log.txt", 60),
    ("CFW/settings.txt", 0),
];

pub const NAME: &str = "cinder-report.txt";

/// The report text. `session` is this run's own log, when there is one to add.
pub fn build(root: Option<&Path>, session: &str) -> String {
    let mut r = format!(
        "Cinder report: installer {} ({} channel) on {} {}\n",
        env!("CARGO_PKG_VERSION"),
        crate::CHANNEL,
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    let Some(root) = root else {
        r.push_str("No Walkman found: none was plugged in, or it is not in mass-storage mode.\n");
        return with_session(r, session);
    };
    let _ = writeln!(r, "Player: {}", root.display());
    let _ = writeln!(r, "{}", device::package_line(root));
    let _ = writeln!(
        r,
        "Update filename: {}",
        match stage::read_ufn(root) {
            Ok(u) if u.eq_ignore_ascii_case("NW_WM_FW") => format!("{u} (as expected)"),
            Ok(u) => format!("{u} (NOT nw_wm_fw: the updater looks for {u}.UPG)"),
            Err(e) => format!("not read ({e})"),
        }
    );
    let _ = writeln!(r, "{}", upg_line(root));

    r.push_str("\n== Top of the drive ==\n");
    let mut entries: Vec<_> = fs::read_dir(root)
        .map(|d| d.flatten().collect())
        .unwrap_or_default();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let Ok(m) = e.metadata() else { continue };
        let size = if m.is_dir() {
            "folder".to_string()
        } else {
            format!("{} bytes", m.len())
        };
        let _ = writeln!(
            r,
            "  {:<32} {:>16}  {}",
            e.file_name().to_string_lossy(),
            size,
            m.modified().map(ago).unwrap_or_default()
        );
    }

    for (name, keep) in FILES {
        let path = root.join(name);
        let _ = write!(r, "\n== {name} ==");
        match fs::read(&path) {
            Err(_) => r.push_str(" (not on the drive)\n"),
            Ok(bytes) => {
                let when = fs::metadata(&path)
                    .and_then(|m| m.modified())
                    .map(ago)
                    .unwrap_or_default();
                let _ = writeln!(r, " ({when})");
                r.push_str(&tail(&String::from_utf8_lossy(&bytes), *keep));
            }
        }
    }
    with_session(r, session)
}

fn with_session(mut r: String, session: &str) -> String {
    if !session.trim().is_empty() {
        r.push_str("\n== This window's log ==\n");
        r.push_str(session);
        if !session.ends_with('\n') {
            r.push('\n');
        }
    }
    r
}

/// Which package `NW_WM_FW.UPG` on the drive is, by comparing it with the ones built in.
fn upg_line(root: &Path) -> String {
    let Ok(bytes) = fs::read(root.join(stage::UPG_NAME)) else {
        return format!("{}: not on the drive", stage::UPG_NAME);
    };
    let installs = crate::payload::PAYLOAD
        .iter()
        .filter(|(n, _, _)| *n == stage::UPG_NAME)
        .map(|(_, owner, b)| (format!("install, {}", owner.trim_start_matches("fw:")), *b));
    let removals = [
        ("uninstall, nw-a50", crate::payload::UNINSTALL_UPG),
        ("uninstall, nw-wm1a", crate::payload::UNINSTALL_UPG_W1),
    ]
    .into_iter()
    .filter_map(|(n, b)| Some((n.to_string(), b?)));
    let which = installs
        .chain(removals)
        .find(|(_, b)| *b == bytes.as_slice())
        .map_or_else(
            || "not a package this installer carries".to_string(),
            |(n, _)| format!("this installer's {n} key"),
        );
    format!("{}: {} bytes, {which}", stage::UPG_NAME, bytes.len())
}

/// The last `keep` lines of `text`, all of it for 0.
fn tail(text: &str, keep: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let from = if keep == 0 {
        0
    } else {
        lines.len().saturating_sub(keep)
    };
    let mut out = String::new();
    if from > 0 {
        let _ = writeln!(out, "  ... {from} earlier lines left out");
    }
    for l in &lines[from..] {
        out.push_str(l);
        out.push('\n');
    }
    out
}

/// "5 min ago", "3 h ago", "2 days ago": relative, so no clock or time zone needs explaining.
fn ago(t: SystemTime) -> String {
    let Ok(d) = SystemTime::now().duration_since(t) else {
        return "dated in the future".to_string();
    };
    match d.as_secs() {
        s if s < 3600 => format!("{} min ago", s / 60),
        s if s < 2 * 86400 => format!("{} h ago", s / 3600),
        s => format!("{} days ago", s / 86400),
    }
}

/// Write the report where the user will find it: the Desktop on Windows, the current folder
/// elsewhere (where the command was typed).
pub fn save(text: &str) -> io::Result<PathBuf> {
    let dir = std::env::var_os("USERPROFILE")
        .filter(|_| cfg!(windows))
        .map(PathBuf::from)
        .map(|home| {
            let desk = home.join("Desktop");
            if desk.is_dir() {
                desk
            } else {
                home
            }
        })
        .map_or_else(std::env::current_dir, Ok)?;
    let path = dir.join(NAME);
    fs::write(&path, text)?;
    Ok(path)
}

/// `--report`: write it and say where.
pub fn cli(root: &Path) -> i32 {
    match save(&build(Some(root), "")) {
        Ok(p) => {
            println!("\n  Report saved: {}", p.display());
            println!(
                "  Attach that file to your GitHub issue. It holds the player's logs, no music."
            );
            0
        }
        Err(e) => {
            eprintln!("\nERROR: could not write the report: {e}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tail_keeps_the_end_and_says_what_it_cut() {
        assert_eq!(tail("a\nb\nc\n", 0), "a\nb\nc\n");
        assert_eq!(
            tail("a\nb\nc\n", 2),
            "  ... 1 earlier lines left out\nb\nc\n"
        );
    }

    #[test]
    fn a_drive_with_nothing_on_it_still_reports() {
        let dir = std::env::temp_dir().join(format!("cinder-report-{}", std::process::id()));
        fs::create_dir_all(dir.join("MUSIC")).unwrap();
        fs::write(
            dir.join(device::LOG_NAME),
            "== cinder-home installer\n== done.\n",
        )
        .unwrap();
        let r = build(Some(&dir), "staged\n");
        fs::remove_dir_all(&dir).unwrap();
        assert!(r.contains("NW_WM_FW.UPG: not on the drive"));
        assert!(r.contains("== exec.txt == (not on the drive)"));
        assert!(r.contains("== done."));
        assert!(r.contains("MUSIC"));
        assert!(r.ends_with("== This window's log ==\nstaged\n"));
    }
}
