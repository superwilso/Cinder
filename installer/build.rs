// build.rs — embed the staged payload into the installer binary.
//
// The installer ships as ONE file from a GitHub release, so everything it needs to write to the
// player has to live inside it: the device binaries, the install/uninstall .UPGs, and the
// component catalogue (embedded rather than duplicated, so deploy/components.conf stays the single
// source of truth for what is optional).
//
// Which channel is embedded is chosen at BUILD time:  CINDER_CHANNEL=dev cargo build …
//
// A MISSING payload is not a build error. It emits an empty table, the installer starts, reports
// exactly which files are absent and exits non-zero. That keeps `cargo build` and `cargo check`
// working in a fresh clone or in CI before cinder-home/build.sh has ever run — the alternative
// (include_bytes! on a missing path) fails with a compiler error that says nothing useful.
use std::path::{Path, PathBuf};
use std::{env, fs};

// (source path, name written to the player's storage root, owning component id, required?)
//
// The component id ties a file to a catalogue entry, so the installer stages only what was
// actually selected — an empty id means "always". `required: false` is for files that legitimately
// exist on one channel only (cinder-probe is dev-channel only), so a stable build does not
// report them as missing.
const FILES: &[(&str, &str, &str, bool)] = &[
    ("dist/{ch}/cinder-home", "cinder-home", "", true),
    // The RE workbench. Dev only, like adb: nothing on a stable player can run it.
    ("dist/{ch}/cinder-probe", "cinder-probe", "", false),
    // Empty id = always staged. These four were catalogue components until 2026-09-12, when the
    // four "choices" that removed the power menu, file transfer, the clock and the unmount helper
    // were taken out of deploy/components.conf: each one's own description said the feature was
    // gone without it. They are part of every install now, and 60 KB between them.
    ("dist/{ch}/cinder-umount", "cinder-umount", "", true),
    ("dist/{ch}/cinder-power", "cinder-power", "", true),
    ("dist/{ch}/cinder-msc", "cinder-msc", "", true),
    ("dist/{ch}/cinder-clock", "cinder-clock", "", true),
    // Built on both channels, so `required` — a stable build missing either is a real defect, not
    // a channel difference. cinder-fm backs the FM signal meter / fast scan / hardware seek;
    // cinder-voltable installs the chosen output volume table on every boot.
    ("dist/{ch}/cinder-fm", "cinder-fm", "fm", true),
    // Empty id = always staged, same as cinder-signature.sh: `voltable` is an ENUM component
    // (stock/wm1a/w1), so there is no "off" that should skip the helper — the curve can be changed
    // on-device later by editing /contents/cinder_voltable.conf, and that needs the binary present.
    ("dist/{ch}/cinder-voltable", "cinder-voltable", "", true),
    // cinder-battery READS the bq24262 charger's registers for the battery screen. Deliberately
    // read-only — unlike cinder-fm it never widens permissions, because writing this chip
    // reprograms a lithium charger. See cinder-home/src/cinder-battery.c.
    (
        "dist/{ch}/cinder-battery",
        "cinder-battery",
        "battery",
        true,
    ),
    (
        "dist/{ch}/cinder-signature.sh",
        "cinder-signature.sh",
        "",
        true,
    ),
    // Always staged, like the signature switcher: `chain` is an enum, and `default` must be able
    // to undo an earlier `clearbass`.
    ("dist/{ch}/cinder-chain.sh", "cinder-chain.sh", "", true),
    // The mono shim (cinder-home/src/cinder-mono.c), loaded through the hagodaemon wrapper.
    (
        "dist/{ch}/libcinder_mono.so",
        "libcinder_mono.so",
        "preload",
        true,
    ),
    // The `preload` component: the hagodaemon wrapper, the boot guard it is never installed
    // without, and the script that puts both on and takes both off (deploy/cinder-preload.sh).
    // Staged only when `preload` is selected. NOT `required` yet: dist/stable is rebuilt at a
    // release and the one committed today (0.3.14) predates all three, so a build from `main`
    // must not fail on them — tools/release.sh is what refuses to cut a release without them.
    (
        "dist/{ch}/cinder-hagowrap",
        "cinder-hagowrap",
        "preload",
        false,
    ),
    (
        "dist/{ch}/cinder-guard.sh",
        "cinder-guard.sh",
        "preload",
        false,
    ),
    (
        "dist/{ch}/cinder-preload.sh",
        "cinder-preload.sh",
        "preload",
        false,
    ),
    // The `bt-aac` component: AAC Bluetooth transmit (cinder-aac/) and the fdk-aac encoder it
    // prefers. Not `required` yet, for the same reason as the preload files: the committed
    // dist/stable predates them, and tools/release.sh is what refuses a release without them.
    (
        "dist/{ch}/libcinder_aac.so",
        "libcinder_aac.so",
        "bt-aac",
        false,
    ),
    (
        "dist/{ch}/libfdk-aac-enc.so",
        "libfdk-aac-enc.so",
        "bt-aac",
        false,
    ),
    // The `atrac` component: ATRAC on any region (cinder-home/src/cinder-atrac.c).
    (
        "dist/{ch}/libcinder_atrac.so",
        "libcinder_atrac.so",
        "atrac",
        false,
    ),
    // The `fmrec` component: FM recording on the jack (cinder-home/src/cinder-fmrec.c).
    (
        "dist/{ch}/libcinder_fmrec.so",
        "libcinder_fmrec.so",
        "fmrec",
        false,
    ),
    // The `wifi` component: the setuid helper (cinder-home/src/cinder-wifi.c), and from wifi/
    // (GPL-2.0, own LICENSE) the radio's four kernel modules, wpa_supplicant, wpa_cli, iw, curl and
    // its CA list, and the scripts that bring the link up. Not `required`: the modules build only where Sony's kernel
    // tree is, so a build without them simply offers no Wi-Fi.
    ("dist/{ch}/cinder-wifi", "cinder-wifi", "wifi", false),
    (
        "dist/{ch}/wifi-compat.ko",
        "cinder-wifi-compat.ko",
        "wifi",
        false,
    ),
    (
        "dist/{ch}/wifi-cfg80211.ko",
        "cinder-wifi-cfg80211.ko",
        "wifi",
        false,
    ),
    (
        "dist/{ch}/wifi-mtk_wmt_wifi_soc.ko",
        "cinder-wifi-mtk_wmt_wifi_soc.ko",
        "wifi",
        false,
    ),
    (
        "dist/{ch}/wifi-wlan_mt.ko",
        "cinder-wifi-wlan_mt.ko",
        "wifi",
        false,
    ),
    (
        "dist/{ch}/wifi-wpa_supplicant",
        "cinder-wifi-wpa_supplicant",
        "wifi",
        false,
    ),
    (
        "dist/{ch}/wifi-wpa_cli",
        "cinder-wifi-wpa_cli",
        "wifi",
        false,
    ),
    ("dist/{ch}/wifi-iw", "cinder-wifi-iw", "wifi", false),
    ("dist/{ch}/wifi-curl", "cinder-wifi-curl", "wifi", false),
    (
        "dist/{ch}/wifi-openssl",
        "cinder-wifi-openssl",
        "wifi",
        false,
    ),
    (
        "dist/{ch}/wifi-shairport-sync-ap2",
        "cinder-wifi-shairport-sync-ap2",
        "wifi",
        false,
    ),
    ("dist/{ch}/wifi-nqptp", "cinder-wifi-nqptp", "wifi", false),
    (
        "dist/{ch}/wifi-cacert.pem",
        "cinder-wifi-cacert.pem",
        "wifi",
        false,
    ),
    (
        "dist/{ch}/wifi-shairport-sync",
        "cinder-wifi-shairport-sync",
        "wifi",
        false,
    ),
    (
        "dist/{ch}/wifi-wifi-up.sh",
        "cinder-wifi-wifi-up.sh",
        "wifi",
        false,
    ),
    (
        "dist/{ch}/wifi-wifi-down.sh",
        "cinder-wifi-wifi-down.sh",
        "wifi",
        false,
    ),
    (
        "dist/{ch}/wifi-dhcp.sh",
        "cinder-wifi-dhcp.sh",
        "wifi",
        false,
    ),
    // The starter list of internet radio stations; copied to the drive only when there is none.
    (
        "dist/{ch}/wifi-cinder_radio.txt",
        "cinder-wifi-cinder_radio.txt",
        "wifi",
        false,
    ),
    // Every advanced switch, commented out: the user renames it to cinder_advanced.conf to use one.
    // Read from deploy/, not dist/: it is text that changes with the source, not a build output.
    (
        "deploy/cinder_advanced.conf.example",
        "cinder_advanced.conf.example",
        "",
        true,
    ),
    // The .UPG the Sony updater actually runs. It MUST land on the player as NW_WM_FW.UPG —
    // that filename is what the device's own "update firmware" flow looks for. One per key: the
    // updater drops a package sealed for another model's key without a word, and Walkman One
    // answers with the NW-WM1A's. `fw:` owners are picked by the firmware, not a component
    // (stage::staged); the Walkman One one is not `required` until a release carries it.
    (
        "dist/{ch}/cinder_home_install.upg",
        "NW_WM_FW.UPG",
        "fw:nw-a50",
        true,
    ),
    (
        "dist/{ch}/cinder_home_install.nw-wm1a.upg",
        "NW_WM_FW.UPG",
        "fw:nw-wm1a",
        false,
    ),
];

// The uninstall package. Deliberately NOT in FILES: it lands under the same name as the install
// one, so staging both would mean writing NW_WM_FW.UPG twice and letting file order decide what
// the player flashes. It is emitted as its own constant and written only by the uninstall action.
//
// It was previously not embedded at all, which is why removing Cinder meant reading UNINSTALL.md
// and copying a file by hand — the uninstall .UPG was built, committed and published as a release
// asset the whole time, and the one program the user actually runs could not reach it.
const UNINSTALL_UPG: &str = "dist/{ch}/cinder_home_uninstall.upg";
const UNINSTALL_UPG_W1: &str = "dist/{ch}/cinder_home_uninstall.nw-wm1a.upg";

fn main() {
    embed_manifest();
    // The channel the installer starts on. Both channels are embedded when both are built (the
    // person installing picks one: the `channel` entry in components.conf); this only says which
    // is chosen before they do.
    let default = env::var("CINDER_CHANNEL").unwrap_or_else(|_| "stable".into());
    println!("cargo:rerun-if-env-changed=CINDER_CHANNEL");

    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let ch_root = manifest.parent().unwrap().join("cinder-home");

    // The player app's own version, which is what the payload installs and what a player reports
    // in cinder_running.txt. The installer's crate version is the release number alone (0.4.1 for
    // every 0.4.1-rcN), so comparing a player against that would call rc3 out of date under rc3.
    let settings = manifest
        .parent()
        .unwrap()
        .join("player/cinder-ui/src/settings.rs");
    println!("cargo:rerun-if-changed={}", settings.display());
    let player_version = fs::read_to_string(&settings)
        .ok()
        .and_then(|t| {
            let i = t.find("macro_rules! cinder_version")?;
            let rest = &t[i..];
            let a = rest.find('"')? + 1;
            let b = a + rest[a..].find('"')?;
            Some(rest[a..b].to_string())
        })
        .unwrap_or_else(|| env::var("CARGO_PKG_VERSION").unwrap());
    println!("cargo:rustc-env=CINDER_PLAYER_VERSION={player_version}");
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());

    let mut src = String::new();
    src.push_str("// generated by build.rs — do not edit\n");
    src.push_str(&format!(
        "pub const DEFAULT_CHANNEL: &str = {:?};\n",
        default
    ));

    // the component catalogue, embedded so the picker and the device installer agree
    let cat = ch_root.join("deploy/components.conf");
    println!("cargo:rerun-if-changed={}", cat.display());
    match fs::read(&cat) {
        Ok(b) => src.push_str(&format!(
            "pub const CATALOGUE: Option<&str> = Some({:?});\n",
            String::from_utf8_lossy(&b)
        )),
        Err(_) => src.push_str("pub const CATALOGUE: Option<&str> = None;\n"),
    }

    // EVERY FILE ONCE. Most of the payload is byte-identical between the channels (the helpers,
    // the libraries, the scripts); only cinder-home, cinder-probe and the packages differ. Each
    // distinct content becomes one `static`, and both channels' tables point at it, so carrying
    // the developer build costs its own files and nothing more.
    let mut blobs: Vec<(Vec<u8>, String)> = Vec::new();
    let mut blob = |p: &Path| -> String {
        let bytes = fs::read(p).unwrap();
        if let Some((_, name)) = blobs.iter().find(|(b, _)| *b == bytes) {
            return name.clone();
        }
        let name = format!("B{}", blobs.len());
        blobs.push((bytes, name.clone()));
        src_blob(&name, p)
    };
    let mut statics = String::new();
    let mut tables = String::new();
    let mut warn = Vec::new();
    for channel in ["stable", "dev"] {
        let mut entries = Vec::new();
        let mut missing = Vec::new();
        for (rel, dest, owner, required) in FILES {
            let rel = rel.replace("{ch}", channel);
            let p = ch_root.join(&rel);
            println!("cargo:rerun-if-changed={}", p.display());
            if p.is_file() {
                let (name, decl) = split_blob(blob(&p));
                statics.push_str(&decl);
                entries.push(format!("        ({:?}, {:?}, {}),\n", dest, owner, name));
            } else if *required {
                missing.push(rel);
            }
        }
        let mut upg = |rel: &str, required: bool, missing: &mut Vec<String>| -> String {
            let rel = rel.replace("{ch}", channel);
            let p = ch_root.join(&rel);
            println!("cargo:rerun-if-changed={}", p.display());
            if p.is_file() {
                let (name, decl) = split_blob(blob(&p));
                statics.push_str(&decl);
                format!("Some({name})")
            } else {
                if required {
                    missing.push(rel);
                }
                "None".to_string()
            }
        };
        let uninstall = upg(UNINSTALL_UPG, true, &mut missing);
        let uninstall_w1 = upg(UNINSTALL_UPG_W1, false, &mut missing);
        // Stable is always embedded, complete or not, so an incomplete build still starts and
        // says what is missing. The developer build is offered only when ALL of it is there:
        // half a channel is not a choice anyone should be shown.
        if channel != "stable" && !missing.is_empty() {
            if channel == default {
                warn.push(format!(
                    "{} file(s) missing for the default channel '{channel}'",
                    missing.len()
                ));
            }
            continue;
        }
        if !missing.is_empty() {
            warn.push(format!(
                "{} payload file(s) missing for channel '{channel}'",
                missing.len()
            ));
        }
        tables.push_str(&format!(
            "    Channel {{\n        name: {channel:?},\n        payload: &[\n{}        ],\n        uninstall_upg: {uninstall},\n        uninstall_upg_w1: {uninstall_w1},\n        missing: &[{}],\n    }},\n",
            entries.concat(),
            missing.iter().map(|m| format!("{m:?}")).collect::<Vec<_>>().join(", ")
        ));
    }
    src.push_str(&statics);
    src.push_str("pub static CHANNELS: &[Channel] = &[\n");
    src.push_str(&tables);
    src.push_str("];\n");

    // NO SONY UPDATER HERE, deliberately. Until 0.3.1 this embedded Sony's
    // SoftwareUpdateTool.exe, WmFwUpdater.dll and the Visual C++ 2010 runtime into every Windows
    // installer, for one thing: the 12-byte vendor SCSI command that reboots the player into its
    // updater. installer/src/stage.rs now sends that command itself on Windows (SCSI pass-through)
    // as it already did on Linux (SG_IO), so no part of Sony's software ships in this binary.

    for w in &warn {
        println!(
            "cargo:warning=cinder-installer: {w} — run tools/fetch_payload.sh (the last \
             release's), or build it: cinder-home/build.sh <channel> && \
             cinder-home/tools/pack_upg.sh <channel>"
        );
    }

    fs::write(out.join("payload.rs"), src).unwrap();
}

/// A blob the first time its content is seen comes back as "NAME\0declaration"; a repeat as just
/// its name, so the caller declares each static once.
fn src_blob(name: &str, p: &Path) -> String {
    format!(
        "{name}\0static {name}: &[u8] = include_bytes!({:?});\n",
        abs(p)
    )
}

fn split_blob(s: String) -> (String, String) {
    match s.split_once('\0') {
        Some((n, d)) => (n.to_string(), d.to_string()),
        None => (s, String::new()),
    }
}

/// Ask the MSVC linker to embed the application manifest.
///
/// Without it the GUI paints in the Windows 95 control style — see the manifest's own comment.
/// Only the MSVC toolchain can do this from the linker alone; the GNU one needs `windres` to
/// compile a resource script, which is a build tool this project does not require anyone to have.
/// The GNU target is used for local `cargo check` only — every published Windows build is MSVC —
/// so a check-only build losing its theming costs nothing.
fn embed_manifest() {
    let mut manifest =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("cinder-installer.manifest");
    println!("cargo:rerun-if-changed={}", manifest.display());
    println!("cargo:rerun-if-env-changed=CINDER_INSTALLER_AS_INVOKER");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    // SCREENSHOT BUILDS ONLY. tools/render_installer_screenshots.sh starts the .exe from WSL, and
    // Windows will not start a requireAdministrator program from there at all: there is no UAC
    // prompt to show, so CreateProcess just fails. This embeds a copy of the manifest that asks for
    // nothing, for a build that only ever runs --screenshots. Nothing in release.yml sets it, and
    // without elevation such a build could not send the upgrade command anyway.
    if env::var_os("CINDER_INSTALLER_AS_INVOKER").is_some_and(|v| !v.is_empty())
        && manifest.is_file()
    {
        let text = fs::read_to_string(&manifest).unwrap();
        let swapped = text.replace("level=\"requireAdministrator\"", "level=\"asInvoker\"");
        assert_ne!(
            swapped, text,
            "CINDER_INSTALLER_AS_INVOKER: the manifest no longer says requireAdministrator"
        );
        let copy = PathBuf::from(env::var("OUT_DIR").unwrap())
            .join("cinder-installer.as-invoker.manifest");
        fs::write(&copy, swapped).unwrap();
        manifest = copy;
    }
    if env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc") {
        // The GNU toolchain has no linker switch for this, but it ships `windres`, which compiles
        // a resource script into a COFF object the linker will take like any other input. Worth
        // doing rather than skipping: a locally cross-built .exe is how the GUI actually gets
        // looked at before a release, and an unthemed one invites a bug report about the styling
        // instead of about the program.
        if !manifest.is_file() {
            return;
        }
        gnu_manifest(&manifest);
        return;
    }
    // A MISSING manifest must not fail the link. `/MANIFESTINPUT:` naming a file that is not
    // there is a hard linker error, so an unguarded pass would turn "the manifest was not
    // committed" into a broken release build whose message points at the linker rather than at
    // the file. Unthemed controls are a cosmetic loss; a failed release build is not.
    if !manifest.is_file() {
        println!(
            "cargo:warning=cinder-installer: {} is missing — building without an application \
                  manifest, so the GUI will use unthemed controls.",
            manifest.display()
        );
        return;
    }
    println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
    println!(
        "cargo:rustc-link-arg-bins=/MANIFESTINPUT:{}",
        manifest.display()
    );
    println!("cargo:rustc-link-arg-bins=/MANIFESTUAC:NO");
}

/// Compile the manifest into a COFF object with `windres` and hand it to the linker.
///
/// Best-effort by design: if `windres` is not installed the build continues without a manifest
/// (unthemed controls, everything else identical) rather than failing. Published builds are MSVC
/// and take the linker path above, so this never gates a release.
fn gnu_manifest(manifest: &Path) {
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let rc = out.join("manifest.rc");
    let obj = out.join("manifest_res.o");

    // 1 = CREATEPROCESS_MANIFEST_RESOURCE_ID, 24 = RT_MANIFEST. Forward slashes: windres reads
    // the path out of the script, where a backslash is an escape.
    let script = format!("1 24 \"{}\"\n", abs(manifest));
    if fs::write(&rc, script).is_err() {
        return;
    }

    for tool in ["x86_64-w64-mingw32-windres", "windres"] {
        let ok = std::process::Command::new(tool)
            .args(["-O", "coff", "-i"])
            .arg(&rc)
            .arg("-o")
            .arg(&obj)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok {
            println!("cargo:rustc-link-arg-bins={}", obj.display());
            return;
        }
    }
    println!(
        "cargo:warning=cinder-installer: windres not found — the GUI will use unthemed \
              controls in this build. Published builds are MSVC and are unaffected."
    );
}

fn abs(p: &Path) -> String {
    fs::canonicalize(p)
        .unwrap_or_else(|_| p.to_path_buf())
        .to_string_lossy()
        .replace('\\', "/")
}
