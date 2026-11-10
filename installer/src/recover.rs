//! Fixing a player that will not start Cinder, or getting Sony's player back for a while, from a PC.
//!
//! Every one of these is a file dropped on (or taken off) the player's drive, which the launcher
//! reads at the next start (`install_cinderhome.sh`, the launcher's flags). They used to be
//! reachable only through `tools/flash.sh` (Linux, sudo) or by making empty files by hand, which is
//! RECOVERY.md steps 1-2. Here they are three buttons, and each says what it will do.

use std::fs;
use std::io;
use std::path::Path;

use crate::device::flags;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fix {
    /// The next start goes to Sony's player, once. Cinder comes back at the start after.
    StockOnce,
    /// Every start goes to Sony's player until [`Fix::TryAgain`].
    TurnOff,
    /// Undo both of those, and the "failed to start twice" latch, and try Cinder at the next start.
    TryAgain,
}

impl Fix {
    pub fn from_word(w: &str) -> Option<Fix> {
        match w {
            "stock-once" => Some(Fix::StockOnce),
            "turn-off" => Some(Fix::TurnOff),
            "try-again" => Some(Fix::TryAgain),
            _ => None,
        }
    }

    /// What happened, in a sentence the page shows when it is done.
    pub fn done(self) -> &'static str {
        match self {
            Fix::StockOnce => {
                "Done. Unplug the player and switch it on: it starts Sony's player this once, and Cinder the time after."
            }
            Fix::TurnOff => {
                "Done. Unplug the player and switch it on: it starts Sony's player every time until you choose \u{201c}Start Cinder again\u{201d}."
            }
            Fix::TryAgain => {
                "Done. Unplug the player and switch it on: it tries Cinder at the next start. Start it unplugged \u{2014} a cable in at start-up also sends it to Sony's player."
            }
        }
    }
}

/// Carry out `fix` on the drive at `root`.
pub fn apply(root: &Path, fix: Fix) -> io::Result<()> {
    match fix {
        Fix::StockOnce => fs::write(root.join(flags::ONCE), b""),
        Fix::TurnOff => fs::write(root.join(flags::OFF), b""),
        Fix::TryAgain => {
            // The one-shot is swept as well: it outranks a clear in the launcher on purpose, so a
            // pending one would spend the very next start on Sony's player, which reads exactly
            // like this having failed (tools/flash.sh --clear-latch does the same).
            for f in [flags::OFF, flags::ONCE, flags::LATCHED, flags::BOOTCOUNT] {
                match fs::remove_file(root.join(f)) {
                    Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
                    _ => {}
                }
            }
            // The latch itself lives on the player's own partition, which a PC cannot reach; this
            // file tells the launcher to clear it.
            fs::write(root.join(flags::CLEAR), b"")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drive(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("cinder-recover-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn stock_once_and_turn_off_drop_their_files() {
        let d = drive("drop");
        apply(&d, Fix::StockOnce).unwrap();
        apply(&d, Fix::TurnOff).unwrap();
        assert!(d.join(flags::ONCE).exists() && d.join(flags::OFF).exists());
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn try_again_clears_everything_and_arms_the_clear() {
        let d = drive("again");
        for f in [flags::OFF, flags::ONCE, flags::LATCHED, flags::BOOTCOUNT] {
            fs::write(d.join(f), b"").unwrap();
        }
        apply(&d, Fix::TryAgain).unwrap();
        for f in [flags::OFF, flags::ONCE, flags::LATCHED, flags::BOOTCOUNT] {
            assert!(!d.join(f).exists(), "{f} left behind");
        }
        assert!(d.join(flags::CLEAR).exists());
        // …and on a clean drive it has nothing to remove and still arms the clear.
        fs::remove_file(d.join(flags::CLEAR)).unwrap();
        apply(&d, Fix::TryAgain).unwrap();
        assert!(d.join(flags::CLEAR).exists());
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn only_the_three_words_are_fixes() {
        assert_eq!(Fix::from_word("try-again"), Some(Fix::TryAgain));
        assert_eq!(Fix::from_word("rm -rf"), None);
    }
}
