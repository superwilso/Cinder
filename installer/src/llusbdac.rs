//! The low-latency USB DAC module, fetched from cinder-llusbdac's release.
//!
//! llusbdac is GPLv2, so Cinder does not ship it: it is built from public sources by
//! github.com/superwilso/cinder-llusbdac's own workflow, and downloaded here only when the
//! `llusbdac` option is ticked. Pinned to one release and checked against its SHA-256, so a
//! changed file on the server fails loudly rather than installing a different kernel module.

use std::fs;
use std::path::Path;

pub const NAME: &str = "llusbdac.ko";
pub const URL: &str =
    "https://github.com/superwilso/cinder-llusbdac/releases/download/v0.1.0/llusbdac.ko";
/// The release's own sha256 (its `llusbdac.ko.sha256` asset). Empty until v0.1.0 is published.
pub const SHA256: &str = "";
/// Generous: the module is ~70 KB.
const LIMIT: usize = 1 << 20;

/// Is the option ticked?
pub fn needed(comps: &[crate::catalogue::Comp]) -> bool {
    comps.iter().any(|c| c.id == "llusbdac" && c.is_on())
}

/// Is the module already on the drive, byte for byte the pinned one?
pub fn on_drive(target: &Path) -> bool {
    fs::read(target.join(NAME)).is_ok_and(|b| matches(&b))
}

fn matches(bytes: &[u8]) -> bool {
    !SHA256.is_empty() && hex(&crate::sony_tables::sha256(bytes)) == SHA256
}

pub fn fetch() -> Result<Vec<u8>, String> {
    if SHA256.is_empty() {
        return Err("this installer was built before the module's release existed".into());
    }
    let bytes = crate::release::get(URL, "", LIMIT)?;
    if !matches(&bytes) {
        return Err(format!("{NAME} from GitHub is not the file Cinder expects"));
    }
    Ok(bytes)
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wrong_file_never_matches() {
        assert!(!matches(b"not a kernel module"));
    }

    #[test]
    fn the_player_checks_the_same_hash() {
        let script = include_str!("../../cinder-home/deploy/install_cinderhome.sh");
        assert!(script.contains(&format!("LLUSBDAC_SHA256=\"{SHA256}\"")));
    }
}
