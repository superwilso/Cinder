//! The two files cinder-rescue needs and cannot ship: MediaTek's Download Agent, and on Windows
//! libwdi's driver installer.
//!
//! A DA is trusted by what is sent to the player, not by the file around it: the MT8590 entry's
//! two stages must hash to the ones verified on the A55 (2026-10-26). mtkclient's current
//! `MTK_DA_V5.bin` (a 2023 build) carries exactly those stages, so it is fetched from there, pinned
//! to a commit; wbrt's v1.0.9 release, which carries an older file with the same stages, is the
//! fallback. The driver installer comes only from wbrt's release, pinned by SHA-256.
//!
//! No HTTP crate: `curl` ships with Windows 10 and later, macOS and nearly every Linux.

use crate::mtk;
use crate::sha256::Sha256;

const MTKCLIENT_URL: &str = "https://raw.githubusercontent.com/bkerler/mtkclient/118a000a83ea31c4f427fe5225abfe964546e05e/mtkclient/Loader/MTK_DA_V5.bin";
const WBRT_URL: &str = "https://github.com/unknown321/wbrt/releases/download/v1.0.9/walkman-backup-restore-tool.v1.0.9.exe";
const WBRT_SHA: &str = "5c376c53190800eccb7413e0c242e529f201abcf1c83c1878226f3e26c9ea5ad";
/// SHA-256 of the DA's stage 1 and stage 2 for the A55 (chip 0x8590, hardware 0xcb01, software 0x0102).
const STAGES_SHA: [&str; 2] = [
    "aa97f0c332e6dbd3afe93ad2deb226dc2693579746008b8cc600e1ce26236117",
    "ecb1c4a155f874b0f919761cce024ce12eb32894581311289aa502cce6ca7099",
];
#[cfg_attr(not(windows), allow(dead_code))]
pub const WDI_SHA: &str = "f41154f97bf58656cfbcd623d67f1d80f7261fe279b39f214478475b462d2a4c";

pub fn sha(b: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(b);
    h.hex()
}

/// Every stored item of a non-solid NSIS installer, inflated: after the 28-byte first header
/// (`…\xEF\xBE\xAD\xDENullsoftInst`) come entries of a little-endian u32 length, whose top bit
/// says "deflated", and that many bytes. The first is the install script; the rest are its files.
pub fn nsis_files(exe: &[u8]) -> Result<Vec<Vec<u8>>, String> {
    let magic = b"\xef\xbe\xad\xdeNullsoftInst";
    let at = exe
        .windows(magic.len())
        .position(|w| w == magic)
        .and_then(|p| p.checked_sub(4))
        .ok_or("not an NSIS installer")?;
    let u32le = |o: usize| u32::from_le_bytes(exe[o..o + 4].try_into().unwrap());
    let end = (at + u32le(at + 24) as usize).min(exe.len());
    let mut p = at + 28;
    let mut out = vec![];
    while p + 4 <= end {
        let n = (u32le(p) & 0x7fff_ffff) as usize;
        let Some(body) = exe.get(p + 4..p + 4 + n).filter(|_| p + 4 + n <= end) else {
            break; // the trailing CRC, not an entry
        };
        out.push(if u32le(p) >> 31 == 1 {
            miniz_oxide::inflate::decompress_to_vec(body)
                .map_err(|e| format!("NSIS entry at {p:#x}: {e:?}"))?
        } else {
            body.to_vec()
        });
        p += 4 + n;
    }
    Ok(out)
}

fn get(url: &str) -> Result<Vec<u8>, String> {
    sayln!("Fetching {url}");
    let o = std::process::Command::new("curl")
        .args(["-sSfL", "--max-time", "600", "-A", "cinder-rescue", url])
        .output()
        .map_err(|e| format!("curl: {e}"))?;
    if !o.status.success() {
        return Err(String::from_utf8_lossy(&o.stderr).trim().to_string());
    }
    Ok(o.stdout)
}

/// The first file in wbrt's v1.0.9 release that `ok` accepts.
pub fn from_wbrt(what: &str, ok: impl Fn(&[u8]) -> bool) -> Result<Vec<u8>, String> {
    let exe = get(WBRT_URL)?;
    if sha(&exe) != WBRT_SHA {
        return Err("wbrt's v1.0.9 release is not the file cinder-rescue expects".into());
    }
    nsis_files(&exe)?
        .into_iter()
        .find(|f| ok(f))
        .ok_or_else(|| format!("{what} is not in wbrt's release"))
}

/// Whether this DA file would send the A55 the two stages it was verified with.
pub fn verified(da: &[u8]) -> bool {
    let Ok((_, entries)) = mtk::parse_da(da) else {
        return false;
    };
    let Ok(stages) = mtk::pick_stages(&entries, 0x8590, 0xcb01, 0x0102) else {
        return false;
    };
    [stages.0, stages.1]
        .iter()
        .zip(STAGES_SHA)
        .all(|(r, want)| {
            da.get(r.offset as usize..r.offset as usize + r.len as usize)
                .is_some_and(|b| sha(b) == want)
        })
}

/// The DA: `--da` if given; else a verified `DA.bin` here or beside this program; else fetched
/// from mtkclient, or failing that from wbrt's release, and saved here as `DA.bin` for next time.
pub fn da(given: Option<&str>) -> Result<Vec<u8>, String> {
    if let Some(p) = given {
        let b = std::fs::read(p).map_err(|e| format!("{p}: {e}"))?;
        if !verified(&b) {
            sayln!(
                "{p} is not the Download Agent cinder-rescue was verified with; using it as asked"
            );
        }
        return Ok(b);
    }
    let beside = std::env::current_exe()
        .ok()
        .and_then(|e| Some(e.parent()?.join("DA.bin")));
    for p in [Some("DA.bin".into()), beside].into_iter().flatten() {
        if let Ok(b) = std::fs::read(&p) {
            if verified(&b) {
                return Ok(b);
            }
            sayln!(
                "{} is not the verified Download Agent; not using it",
                p.display()
            );
        }
    }
    sayln!("No DA.bin here: fetching MediaTek's Download Agent");
    let b = match get(MTKCLIENT_URL) {
        Ok(b) if verified(&b) => b,
        r => {
            sayln!(
                "mtkclient's copy {}; trying wbrt's release",
                r.map_or_else(
                    |e| format!("could not be fetched ({e})"),
                    |_| "has changed".into()
                )
            );
            from_wbrt("MediaTek's Download Agent", verified)?
        }
    };
    match std::fs::write("DA.bin", &b) {
        Ok(()) => sayln!("Saved it as DA.bin, so the next run needs no download"),
        Err(e) => sayln!("Could not save DA.bin here ({e}); the next run downloads it again"),
    }
    Ok(b)
}

/// Windows: install the WinUSB driver for the player in download mode, as wbrt does, with libwdi's
/// `wdi-simple` from wbrt's release. Windows asks for administrator rights (UAC) for that program
/// alone, so cinder-rescue itself runs as a normal user.
#[cfg(windows)]
pub fn driver() -> Result<(), String> {
    let exe = from_wbrt("libwdi's driver installer (wdi-simple.exe)", |f| {
        sha(f) == WDI_SHA
    })?;
    let dir = std::env::temp_dir().join("cinder-rescue-driver");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = dir.join("wdi-simple.exe");
    std::fs::write(&path, exe).map_err(|e| format!("{}: {e}", path.display()))?;
    sayln!("Installing the WinUSB driver for USB 0E8D:2000 (MT65xx Preloader). Windows asks for administrator rights; this takes up to two minutes");
    // PowerShell: `" is a quote inside a "…" string, '' a quote inside a '…' one. The program sees
    // an ordinary Windows command line: --name "cinder-rescue (…)" … --dest "C:\…".
    let args = format!(
        "--name `\"cinder-rescue (MT65xx Preloader)`\" --vid 0x0E8D --pid 0x2000 --type 0 --timeout 120000 --dest `\"{}`\"",
        dir.join("driver").display()
    );
    let ps = format!(
        "$p = Start-Process -FilePath '{}' -ArgumentList \"{args}\" -Verb RunAs -Wait -PassThru; exit $p.ExitCode",
        path.display().to_string().replace('\'', "''")
    );
    let st = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &ps])
        .status()
        .map_err(|e| format!("powershell: {e}"))?;
    if !st.success() {
        return Err(format!(
            "the driver installer did not finish ({st}); if Windows asked for permission, say Yes"
        ));
    }
    sayln!("Driver installed.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nsis_entries_come_out_whole_stored_or_deflated() {
        let (a, b) = (b"the script".to_vec(), vec![7u8; 5000]);
        let mut exe = vec![0u8; 512]; // the stub
        let mut data = vec![];
        data.extend_from_slice(&(a.len() as u32).to_le_bytes());
        data.extend_from_slice(&a);
        let z = miniz_oxide::deflate::compress_to_vec(&b, 6);
        data.extend_from_slice(&(z.len() as u32 | 1 << 31).to_le_bytes());
        data.extend_from_slice(&z);
        exe.extend_from_slice(&0u32.to_le_bytes());
        exe.extend_from_slice(b"\xef\xbe\xad\xdeNullsoftInst");
        exe.extend_from_slice(&0u32.to_le_bytes());
        exe.extend_from_slice(&((28 + data.len()) as u32).to_le_bytes());
        exe.extend_from_slice(&data);
        exe.extend_from_slice(&0xdeadu32.to_le_bytes()); // CRC
        assert_eq!(nsis_files(&exe).unwrap(), vec![a, b]);
        assert!(nsis_files(b"MZ nothing here").is_err());
    }
}
