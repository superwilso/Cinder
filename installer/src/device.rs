//! Finding the player, and reading back what is already on it.
//!
//! The storage root the Walkman exposes over USB mass storage IS the device's `/contents`. That
//! is what makes state detection possible without any device-side cooperation: the install script
//! logs to `/contents/cinder_home_install.log`, and the answers it read are in
//! `/contents/cinder_components.conf`. Both are plain files on the drive the host just mounted.

use std::fs;
use std::path::{Path, PathBuf};

/// Written by `install_cinderhome.sh` and `uninstall_cinderhome.sh` on the device.
pub const LOG_NAME: &str = "cinder_home_install.log";
pub const CONF_NAME: &str = "cinder_components.conf";
/// cinder-home's own log, which the launcher also writes one line to before every hand-over to
/// Sony's player (0.3.10 on).
pub const HOME_LOG: &str = "cinderhome.log";

/// Walkman One keeps its settings and its per-boot log in `CFW/` at the top of the drive
/// (`settings.txt`, `boot_log.txt` — `analysis/RE_walkmanone_extract.md`). Nothing else writes there.
pub const W1_DIR: &str = "CFW";

/// Shown whenever the drive carries Walkman One's folder.
///
/// WHY THIS IS A WARNING AND NOT A REFUSAL. A player running Walkman One reports another model's
/// KAS key (an NW-A55 under W1 answers with the NW-WM1A one), and Sony's updater drops a package
/// sealed for a different key WITHOUT A WORD: it boots, fails, and restarts into Walkman One. That
/// is the whole of the report "it boots into the updater for a second, but reboots back into Mr.
/// Walkman" (r/walkman, 2026-09-21). But the folder is user data on the drive and survives a
/// revert to stock, so its presence cannot tell "running W1" from "ran it once" — only the person
/// holding the player can.
pub const W1_WARNING: &str = "Walkman One files (CFW folder) are on this player. If it is still \
running Walkman One, this package will not install: the player restarts from its updater without \
installing and without a message. Revert to stock firmware first. If you already have, carry on.";

/// Does the drive carry Walkman One's folder? See [`W1_WARNING`] for what this can and cannot say.
pub fn walkman_one_marks(root: &Path) -> bool {
    let cfw = root.join(W1_DIR);
    cfw.join("settings.txt").is_file() || cfw.join("boot_log.txt").is_file()
}

/// Why the LAST boot handed over to Sony's player, if it did.
///
/// The launcher writes `cinderhome-launch: <reason> -> stock` before every escape, and cinder-home
/// writes `main: start` when it comes up. A breadcrumb AFTER the last start is the most recent
/// boot's story; one before it was followed by a boot that did start Cinder, so it is history.
///
/// This is the answer issue #16 had no way to give: an install log full of successes, a player on
/// Sony's UI, and nothing to say which rung sent it there.
pub fn last_boot_to_stock(text: &str) -> Option<String> {
    let mut reason = None;
    for line in text.lines() {
        let line = line.trim();
        if line.contains("main: start") {
            reason = None;
        } else if let Some(rest) = line.strip_prefix("cinderhome-launch: ") {
            if let Some(r) = rest.strip_suffix(" -> stock") {
                reason = Some(r.trim().to_string());
            }
        }
    }
    reason
}

/// A Walkman's storage root has these at the top level. `DevIcon.fil` alone is a strong enough
/// signal; the MUSIC+PC_Application pair covers units where the icon file was deleted.
pub fn looks_like_walkman(root: &Path) -> bool {
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

pub fn find_players() -> Vec<PathBuf> {
    candidate_roots()
        .into_iter()
        .filter(|p| p.is_dir() && looks_like_walkman(p))
        .collect()
}

// ── what the player is currently running ───────────────────────────────────────────────────

/// The outcome of the last install/uninstall the device ran, as recorded in its own log.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LastRun {
    /// The device applied an install and its final sanity gate passed.
    Installed,
    /// The device applied the uninstall package; it is back on the stock Qt UI.
    Uninstalled,
    /// The install script's sanity gate failed and reverted the `.appcfg` to stock. The device
    /// boots — this is the safe failure, not a brick — but Cinder is NOT running.
    Aborted,
    /// A banner with no terminator after it. The package was interrupted, or is mid-flight.
    Unfinished,
}

impl LastRun {
    pub fn describe(self) -> &'static str {
        match self {
            LastRun::Installed => "Cinder is installed",
            LastRun::Uninstalled => "Cinder was removed — the player is on the stock UI",
            LastRun::Aborted => "the last install aborted safely and reverted to stock",
            LastRun::Unfinished => "the last package did not finish",
        }
    }
}

#[derive(Clone, Default)]
pub struct Installed {
    /// What the device's own log says happened last. `None` = no log, so Cinder has never been
    /// installed from this drive.
    pub last: Option<LastRun>,
    /// The banner line of that last run, which carries the device's date.
    pub when: Option<String>,
    /// From the `# channel:` / `# installer:` stamps in `cinder_components.conf`.
    pub channel: Option<String>,
    pub installer_version: Option<String>,
    /// The saved conf itself, for re-applying the user's choices on an update.
    pub conf: Option<String>,
    /// Payload files still sitting in the storage root from a previous staging. The device install
    /// script says these are "safe to delete once cinder-home is confirmed" and nothing ever does,
    /// so they accumulate.
    pub leftovers: Vec<String>,
    /// Walkman One's folder is on the drive. See [`W1_WARNING`].
    pub walkman_one: bool,
    /// The reason the launcher gave for the last boot landing on Sony's player, if it did.
    pub last_boot_stock: Option<String>,
}

impl Installed {
    /// Is there a reason to offer Update and Uninstall rather than just Install?
    pub fn present(&self) -> bool {
        matches!(self.last, Some(LastRun::Installed) | Some(LastRun::Unfinished))
    }

    /// The one line worth saying under the status, most urgent first: Walkman One (the install
    /// cannot work), then why the last boot went to Sony's player.
    pub fn advisory(&self) -> Option<String> {
        if self.walkman_one {
            return Some(W1_WARNING.to_string());
        }
        self.last_boot_stock
            .as_ref()
            .map(|r| format!("The last start went to Sony's player because: {r}."))
    }

    /// One line for the top of the window.
    pub fn summary(&self) -> String {
        match self.last {
            None => "No Cinder install found on this player.".into(),
            Some(k) => {
                let mut s = k.describe().to_string();
                if let Some(v) = &self.installer_version {
                    s.push_str(&format!(" (installer {v}"));
                    if let Some(c) = &self.channel {
                        s.push_str(&format!(", {c} channel"));
                    }
                    s.push(')');
                } else if let Some(c) = &self.channel {
                    s.push_str(&format!(" ({c} channel)"));
                }
                if let Some(w) = &self.when {
                    s.push_str(&format!(" — {w}"));
                }
                s
            }
        }
    }
}

/// Read the device's own log and conf off the mounted drive.
pub fn read_installed(root: &Path, payload_names: &[&str]) -> Installed {
    let mut st = Installed::default();

    if let Ok(text) = fs::read_to_string(root.join(LOG_NAME)) {
        let (last, when) = parse_log(&text);
        st.last = last;
        st.when = when;
    }
    if let Ok(text) = fs::read_to_string(root.join(CONF_NAME)) {
        let (ch, ver) = crate::catalogue::saved_stamp(&text);
        st.channel = ch;
        st.installer_version = ver;
        st.conf = Some(text);
    }
    for name in payload_names {
        if root.join(name).is_file() {
            st.leftovers.push((*name).to_string());
        }
    }
    st.walkman_one = walkman_one_marks(root);
    // Lossy: the log carries the odd non-UTF-8 byte (a tag string, a raw register dump), and one
    // of those must not cost the whole reading.
    if let Ok(bytes) = fs::read(root.join(HOME_LOG)) {
        st.last_boot_stock = last_boot_to_stock(&String::from_utf8_lossy(&bytes));
    }
    st
}


/// Reduce the log to the outcome of its LAST session.
///
/// The log is append-only across every install and uninstall the device has ever run, so reading
/// it top-down and taking the first answer reports the state from months ago. Each session starts
/// with a `== cinder-home installer`/`UNINSTALL` banner and ends with a `== done.` or
/// `== install ABORTED` line; anything after the final banner belongs to that session.
pub fn parse_log(text: &str) -> (Option<LastRun>, Option<String>) {
    let mut banner: Option<(&str, bool)> = None; // (line, is_uninstall)
    let mut outcome: Option<LastRun> = None;

    for raw in text.lines() {
        let line = raw.trim();
        if let Some(rest) = line.strip_prefix("== cinder-home ") {
            // A new session resets the outcome — a `done` from the previous one is not this one's.
            let is_uninstall = rest.starts_with("UNINSTALL");
            banner = Some((line, is_uninstall));
            outcome = Some(LastRun::Unfinished);
        } else if line.starts_with("== install ABORTED") {
            outcome = Some(LastRun::Aborted);
        } else if line.starts_with("== done.") {
            outcome = Some(match banner {
                Some((_, true)) => LastRun::Uninstalled,
                _ => LastRun::Installed,
            });
        }
    }

    let when = banner.map(|(l, _)| {
        // "== cinder-home installer  Wed Sep 10 12:00:00 2026" -> the date half.
        l.trim_start_matches("== cinder-home ")
            .trim_start_matches("installer")
            .trim_start_matches("UNINSTALL")
            .trim()
            .to_string()
    });
    (outcome, when.filter(|w| !w.is_empty()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walkman_detection_needs_the_markers() {
        let tmp = std::env::temp_dir().join(format!("cinder-inst-test-{}", std::process::id()));
        let _ = fs::create_dir_all(tmp.join("MUSIC"));
        assert!(!looks_like_walkman(&tmp), "MUSIC alone is not enough");
        let _ = fs::create_dir_all(tmp.join("PC_Application"));
        assert!(looks_like_walkman(&tmp));
        let _ = fs::remove_dir_all(&tmp);
    }

    /// The two lines copied from a real device log (NW-A55, 2026-09-23) — not written by hand, per
    /// the rule that a parser fixture must come from a real boot.
    const START: &str = "[cinder-home]    0.000 main: start";
    const CABLE: &str = "cinderhome-launch: a USB cable was connected at boot (rung 0; /data/cinder/cable_escape_off opts out) -> stock";

    #[test]
    fn a_breadcrumb_after_the_last_start_is_the_last_boot() {
        let log = format!("cinderhome-launch: volume curve: stock — keeping the table the boot script loaded\n{START}\nlater lines\n{CABLE}\n");
        assert_eq!(
            last_boot_to_stock(&log).as_deref(),
            Some("a USB cable was connected at boot (rung 0; /data/cinder/cable_escape_off opts out)")
        );
        // A launcher line that is NOT an escape is not a reason.
        assert_eq!(last_boot_to_stock("cinderhome-launch: volume curve: stock — keeping the table the boot script loaded\n"), None);
    }

    #[test]
    fn a_breadcrumb_followed_by_a_start_is_history() {
        let log = format!("{CABLE}\n{START}\n");
        assert_eq!(last_boot_to_stock(&log), None);
        assert_eq!(last_boot_to_stock(""), None);
    }

    #[test]
    fn walkman_one_is_read_from_its_folder_and_outranks_the_boot_reason() {
        let tmp = std::env::temp_dir().join(format!("cinder-inst-w1-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        let _ = fs::create_dir_all(tmp.join("MUSIC"));
        let _ = fs::create_dir_all(tmp.join("PC_Application"));
        fs::write(tmp.join(HOME_LOG), format!("{START}\n{CABLE}\n")).unwrap();
        let st = read_installed(&tmp, &[]);
        assert!(!st.walkman_one, "no CFW folder, no warning");
        assert!(st.advisory().unwrap().contains("USB cable"));

        // An empty CFW folder is not Walkman One's; its settings file is.
        let _ = fs::create_dir_all(tmp.join(W1_DIR));
        assert!(!read_installed(&tmp, &[]).walkman_one);
        fs::write(tmp.join(W1_DIR).join("settings.txt"), "ADB=1\n").unwrap();
        let st = read_installed(&tmp, &[]);
        assert!(st.walkman_one);
        assert_eq!(st.advisory().as_deref(), Some(W1_WARNING), "W1 outranks the boot reason");
        let _ = fs::remove_dir_all(&tmp);
    }

    const INSTALL: &str = "\
================================================================
== cinder-home installer  Wed Sep 10 12:00:00 2026
busybox: /xbin/busybox
cleared prior disable flags (fresh install = enabled)
== done. reboot to normal; appmgr launches cinder-home as the Home app. ==
";

    #[test]
    fn a_successful_install_reads_as_installed() {
        let (k, when) = parse_log(INSTALL);
        assert_eq!(k, Some(LastRun::Installed));
        assert_eq!(when.as_deref(), Some("Wed Sep 10 12:00:00 2026"));
    }

    /// The log is append-only. Installing, then uninstalling, must report UNINSTALLED — reading
    /// top-down and stopping at the first `== done.` reports a state two sessions stale, which is
    /// exactly the wrong thing to show above an "Update" button.
    #[test]
    fn the_last_session_wins_over_every_earlier_one() {
        let text = format!(
            "{INSTALL}\
================================================================
== cinder-home UNINSTALL  Thu Sep 11 09:30:00 2026
== done. reboot to normal -> stock Qt UI. ==
"
        );
        let (k, when) = parse_log(&text);
        assert_eq!(k, Some(LastRun::Uninstalled));
        assert_eq!(when.as_deref(), Some("Thu Sep 11 09:30:00 2026"));
    }

    /// The device's sanity gate reverting to stock is a SAFE failure, but it is still a failure:
    /// the player boots the Qt UI and the user is owed that fact, not a green tick.
    #[test]
    fn a_reverted_install_is_not_reported_as_installed() {
        let text = "\
== cinder-home installer  Wed Sep 10 12:00:00 2026
sanity: cinder-home not executable
!! SANITY FAILED — reverting .appcfg to stock so the device boots normally.
== install ABORTED safely; device will boot the stock UI. ==
";
        assert_eq!(parse_log(text).0, Some(LastRun::Aborted));
    }

    #[test]
    fn a_banner_with_no_terminator_is_unfinished() {
        let text = "== cinder-home installer  Wed Sep 10 12:00:00 2026\nbusybox: /xbin/busybox\n";
        assert_eq!(parse_log(text).0, Some(LastRun::Unfinished));
    }

    #[test]
    fn no_log_at_all_means_never_installed() {
        assert_eq!(parse_log("").0, None);
        assert!(!Installed::default().present());
    }

    /// An aborted or uninstalled player must not be offered "Update" — there is nothing there to
    /// update, and the only correct action is a fresh install.
    #[test]
    fn update_is_only_offered_when_something_is_actually_there() {
        let mk = |k| Installed { last: Some(k), ..Default::default() };
        assert!(mk(LastRun::Installed).present());
        assert!(mk(LastRun::Unfinished).present());
        assert!(!mk(LastRun::Uninstalled).present());
        assert!(!mk(LastRun::Aborted).present());
    }

    #[test]
    fn the_summary_names_the_version_and_channel_when_known() {
        let st = Installed {
            last: Some(LastRun::Installed),
            when: Some("Wed Sep 10".into()),
            channel: Some("stable".into()),
            installer_version: Some("0.2.0".into()),
            ..Default::default()
        };
        let s = st.summary();
        assert!(s.contains("0.2.0"), "{s}");
        assert!(s.contains("stable"), "{s}");
        assert!(s.contains("Wed Sep 10"), "{s}");
    }
}
