//! cinder-installer — pick which parts of Cinder to install, then stage them onto the Walkman.
//!
//! This is the artefact end users run. It is a console program on purpose: it has to work over
//! RDP, in a VM, from a rescue shell, and on a machine where nothing else is installed.
//!
//! WHAT IT DOES
//!   1. finds the player's storage (it mounts as a plain USB drive)
//!   2. asks which optional components to install — the catalogue is embedded from
//!      cinder-home/deploy/components.conf, so it can never drift from what the device installer
//!      actually understands
//!   3. writes the answers as cinder_components.conf and copies the staged files to the drive root
//!   4. writes the install package as NW_WM_FW.UPG
//!
//! It does NOT flash anything. The last step is done by the player itself, from its own menu,
//! which is what keeps this program unable to brick a device on its own.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

include!(concat!(env!("OUT_DIR"), "/payload.rs"));

const CONF_NAME: &str = "cinder_components.conf";

// ── the component catalogue ────────────────────────────────────────────────────────────────

#[derive(Clone)]
enum Kind {
    Bool,
    Enum(Vec<String>),
}

#[derive(Clone)]
struct Comp {
    id: String,
    var: String,
    kind: Kind,
    default: String,
    title: String,
    desc: String,
    value: String,
}

impl Comp {
    fn allowed(&self) -> Vec<String> {
        match &self.kind {
            Kind::Bool => vec!["0".into(), "1".into()],
            Kind::Enum(v) => v.clone(),
        }
    }
    fn valid(&self, v: &str) -> bool {
        self.allowed().iter().any(|a| a == v)
    }
    fn cycle(&mut self) {
        let a = self.allowed();
        let i = a.iter().position(|x| *x == self.value).unwrap_or(0);
        self.value = a[(i + 1) % a.len()].clone();
    }
    fn render(&self) -> String {
        match self.kind {
            Kind::Bool => if self.value == "1" { "[x]".into() } else { "[ ]".into() },
            Kind::Enum(_) => format!("<{}>", self.value),
        }
    }
}

/// Parse deploy/components.conf. Format: `id | VARNAME | type | default | title`, with indented
/// continuation lines forming the description. Same grammar tools/configure.sh reads.
fn parse_catalogue(text: &str) -> Result<Vec<Comp>, String> {
    let mut out: Vec<Comp> = Vec::new();
    for line in text.lines() {
        let t = line.trim_start();
        if t.starts_with('#') || t.is_empty() {
            continue;
        }
        let indented = line.starts_with(' ') || line.starts_with('\t');
        if !indented && line.contains('|') {
            let f: Vec<&str> = line.split('|').map(|s| s.trim()).collect();
            if f.len() < 5 {
                return Err(format!("malformed catalogue line: {line}"));
            }
            let kind = if f[2] == "bool" {
                Kind::Bool
            } else if let Some(rest) = f[2].strip_prefix("enum:") {
                Kind::Enum(rest.split(',').map(|s| s.trim().to_string()).collect())
            } else {
                return Err(format!("unknown component type '{}'", f[2]));
            };
            out.push(Comp {
                id: f[0].into(),
                var: f[1].into(),
                kind,
                default: f[3].into(),
                title: f[4].into(),
                desc: String::new(),
                value: f[3].into(),
            });
        } else if indented {
            if let Some(last) = out.last_mut() {
                last.desc.push_str(t);
                last.desc.push('\n');
            }
        }
    }
    if out.is_empty() {
        return Err("catalogue contained no components".into());
    }
    for c in &out {
        if !c.valid(&c.default) {
            return Err(format!("default '{}' invalid for '{}'", c.default, c.id));
        }
    }
    Ok(out)
}

// ── finding the player ─────────────────────────────────────────────────────────────────────

/// A Walkman's storage root has these at the top level. `DevIcon.fil` alone is a strong enough
/// signal; the MUSIC+PC_Application pair covers units where the icon file was deleted.
fn looks_like_walkman(root: &Path) -> bool {
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

fn find_players() -> Vec<PathBuf> {
    candidate_roots()
        .into_iter()
        .filter(|p| p.is_dir() && looks_like_walkman(p))
        .collect()
}

// ── console helpers ────────────────────────────────────────────────────────────────────────

fn prompt(msg: &str) -> String {
    print!("{msg}");
    let _ = io::stdout().flush();
    let mut s = String::new();
    if io::stdin().read_line(&mut s).is_err() {
        return String::new();
    }
    s.trim().to_string()
}

fn show(comps: &[Comp], target: &Path) {
    println!();
    println!("  Cinder installer  (channel: {CHANNEL})");
    println!("  player: {}", target.display());
    println!("  ------------------------------------------------------------");
    for (i, c) in comps.iter().enumerate() {
        println!("   {:>2}  {:<7} {:<42} {}", i + 1, c.render(), c.title, c.id);
    }
    println!("  ------------------------------------------------------------");
    println!("   <number> toggle/cycle   ?<number> describe   i install   q quit");
}

// ── writing ────────────────────────────────────────────────────────────────────────────────

fn conf_text(comps: &[Comp]) -> String {
    let mut s = String::new();
    s.push_str("# cinder_components.conf - generated by cinder-installer; do not edit by hand.\n");
    s.push_str("# Read (never sourced) by install_cinderhome.sh on the device.\n");
    s.push_str(&format!("# channel: {CHANNEL}\n"));
    for c in comps {
        s.push_str(&format!("\n# {}\n{}={}\n", c.title, c.var, c.value));
    }
    s
}

/// Is the component that owns this payload file switched on? An empty owner means the file is
/// part of every install (the app itself, the probe, the .UPG). A bool component set to "0" is
/// off; an enum component is always staged, because its switcher has to be present on the device
/// for the choice to be changeable later.
fn selected(comps: &[Comp], owner: &str) -> bool {
    if owner.is_empty() {
        return true;
    }
    comps
        .iter()
        .find(|c| c.id == owner)
        .map(|c| c.value != "0")
        .unwrap_or(true)
}

fn staged_count(comps: &[Comp]) -> usize {
    PAYLOAD.iter().filter(|(_, o, _)| selected(comps, o)).count()
}

fn install(comps: &[Comp], target: &Path) -> io::Result<()> {
    println!("\n  writing to {} ...", target.display());
    for (name, owner, bytes) in PAYLOAD {
        if !selected(comps, owner) {
            println!("    {:<22} {:>9}", name, "not selected");
            continue;
        }
        let dest = target.join(name);
        fs::write(&dest, bytes)?;
        println!("    {:<22} {:>9} bytes", name, bytes.len());
    }
    let conf = target.join(CONF_NAME);
    fs::write(&conf, conf_text(comps))?;
    println!("    {:<22} {:>9} bytes", CONF_NAME, conf_text(comps).len());
    Ok(())
}

// ── main ───────────────────────────────────────────────────────────────────────────────────

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut explicit: Option<PathBuf> = None;
    let mut assume_yes = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-y" | "--yes" => assume_yes = true,
            "-h" | "--help" => {
                println!("cinder-installer [--yes] [<drive or mount point>]");
                println!("  Stages Cinder onto the Walkman. Does not flash — the player does that");
                println!("  itself, from Settings, after you eject it.");
                return;
            }
            other => explicit = Some(PathBuf::from(other)),
        }
        i += 1;
    }

    println!("Cinder installer — channel {CHANNEL}");

    if !MISSING.is_empty() {
        eprintln!("\nERROR: this build is incomplete — the following payload files were missing");
        eprintln!("when it was compiled:");
        for m in MISSING {
            eprintln!("    {m}");
        }
        eprintln!("\nBuild them first:  cinder-home/build.sh {CHANNEL}");
        eprintln!("                   cinder-home/tools/pack_upg.sh {CHANNEL}");
        std::process::exit(2);
    }

    let catalogue = match CATALOGUE {
        Some(c) => c,
        None => {
            eprintln!("ERROR: no component catalogue embedded (deploy/components.conf missing).");
            std::process::exit(2);
        }
    };
    let mut comps = match parse_catalogue(catalogue) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("ERROR: {e}");
            std::process::exit(2);
        }
    };

    // ── locate the player ──
    let target = match explicit {
        Some(p) => {
            if !p.is_dir() {
                eprintln!("ERROR: {} is not a directory.", p.display());
                std::process::exit(1);
            }
            if !looks_like_walkman(&p) {
                eprintln!("WARNING: {} does not look like a Walkman's storage", p.display());
                eprintln!("         (no DevIcon.fil, no MUSIC + PC_Application).");
                if !assume_yes && prompt("         Use it anyway? [y/N] ").to_lowercase() != "y" {
                    return;
                }
            }
            p
        }
        None => {
            let found = find_players();
            match found.len() {
                0 => {
                    eprintln!("\nERROR: no Walkman found.");
                    eprintln!("  Connect the player by USB and set it to mass-storage mode, then");
                    eprintln!("  run this again. If it is mounted somewhere unusual, pass the path:");
                    eprintln!("      cinder-installer D:\\");
                    std::process::exit(1);
                }
                1 => found[0].clone(),
                _ => {
                    println!("\nSeveral candidates found:");
                    for (n, p) in found.iter().enumerate() {
                        println!("   {}  {}", n + 1, p.display());
                    }
                    let a = prompt("Which one? ");
                    match a.parse::<usize>() {
                        Ok(n) if n >= 1 && n <= found.len() => found[n - 1].clone(),
                        _ => {
                            eprintln!("Not a listed choice — stopping.");
                            std::process::exit(1);
                        }
                    }
                }
            }
        }
    };

    // ── pick ──
    if assume_yes {
        if let Err(e) = install(&comps, &target) {
            eprintln!("\nERROR: {e}");
            std::process::exit(1);
        }
        print_next_steps();
        return;
    }

    loop {
        show(&comps, &target);
        let a = prompt("  > ");
        if a.is_empty() {
            continue;
        }
        if a == "q" || a == "Q" {
            println!("  nothing written.");
            return;
        }
        if a == "i" || a == "I" {
            break;
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

    // ── confirm, then write ──
    println!("\n  About to write to {}:", target.display());
    for c in &comps {
        println!("    {:<10} {}", c.id, c.value);
    }
    println!(
        "\n  {} files plus {} will be copied to the player's storage root.",
        staged_count(&comps),
        CONF_NAME
    );
    println!("  Nothing is flashed by this program — the player does that itself, later.");
    if prompt("\n  Proceed? [y/N] ").to_lowercase() != "y" {
        println!("  nothing written.");
        return;
    }

    if let Err(e) = install(&comps, &target) {
        eprintln!("\nERROR: {e}");
        eprintln!("The player may now hold a partial copy. Re-run before flashing.");
        std::process::exit(1);
    }
    print_next_steps();
}

fn print_next_steps() {
    println!("\n  Done. To finish the install, on the PLAYER:");
    println!("    1. Eject the drive safely, then unplug it.");
    println!("    2. Settings -> Device Settings -> Update -> follow the prompts.");
    println!("    3. It reboots into Cinder.");
    println!("\n  If a boot ever goes wrong: hold the player's USB cable in at power-on to get");
    println!("  the stock player back, and see RECOVERY.md.");
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
# a comment
power | CINDER_POWER | bool | 1 | Power off / Restart menu
    Installs cinder-power.
    Second line.

signature | CINDER_SIGNATURE | enum:stock,pv1,pv2 | stock | Audio sound signature
    Patches three bytes.
";

    #[test]
    fn parses_both_kinds() {
        let c = parse_catalogue(SAMPLE).unwrap();
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].id, "power");
        assert_eq!(c[0].var, "CINDER_POWER");
        assert!(matches!(c[0].kind, Kind::Bool));
        assert_eq!(c[0].value, "1");
        assert_eq!(c[1].allowed(), vec!["stock", "pv1", "pv2"]);
    }

    #[test]
    fn description_lines_attach_to_owner() {
        let c = parse_catalogue(SAMPLE).unwrap();
        assert!(c[0].desc.contains("Installs cinder-power."));
        assert!(c[0].desc.contains("Second line."));
        assert!(!c[0].desc.contains("Patches three bytes."));
        assert!(c[1].desc.contains("Patches three bytes."));
    }

    #[test]
    fn cycle_wraps_and_stays_valid() {
        let mut c = parse_catalogue(SAMPLE).unwrap();
        let sig = &mut c[1];
        assert_eq!(sig.value, "stock");
        sig.cycle();
        assert_eq!(sig.value, "pv1");
        sig.cycle();
        assert_eq!(sig.value, "pv2");
        sig.cycle();
        assert_eq!(sig.value, "stock");
        assert!(sig.valid(&sig.value));
    }

    #[test]
    fn bool_toggles() {
        let mut c = parse_catalogue(SAMPLE).unwrap();
        assert_eq!(c[0].render(), "[x]");
        c[0].cycle();
        assert_eq!(c[0].value, "0");
        assert_eq!(c[0].render(), "[ ]");
    }

    #[test]
    fn generated_conf_is_parseable_key_values() {
        let c = parse_catalogue(SAMPLE).unwrap();
        let t = conf_text(&c);
        assert!(t.contains("CINDER_POWER=1"));
        assert!(t.contains("CINDER_SIGNATURE=stock"));
        // every non-comment, non-blank line must be a bare KEY=VALUE — the device side greps
        // for exactly that shape and ignores anything else.
        for line in t.lines().filter(|l| !l.starts_with('#') && !l.trim().is_empty()) {
            let (k, v) = line.split_once('=').expect("KEY=VALUE");
            assert!(k.chars().all(|ch| ch.is_ascii_uppercase() || ch == '_'), "{k}");
            assert!(v.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_'), "{v}");
        }
    }

    #[test]
    fn rejects_bad_catalogue() {
        assert!(parse_catalogue("").is_err());
        assert!(parse_catalogue("a | B | wat | 1 | T").is_err());
        assert!(parse_catalogue("a | B | bool | 7 | T").is_err());
    }

    #[test]
    fn walkman_detection_needs_the_markers() {
        let tmp = std::env::temp_dir().join(format!("cinder-inst-test-{}", std::process::id()));
        let _ = fs::create_dir_all(tmp.join("MUSIC"));
        assert!(!looks_like_walkman(&tmp), "MUSIC alone is not enough");
        let _ = fs::create_dir_all(tmp.join("PC_Application"));
        assert!(looks_like_walkman(&tmp));
        let _ = fs::remove_dir_all(&tmp);
    }
}
