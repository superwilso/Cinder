//! The component catalogue: what an install is allowed to leave out, and how the answer is
//! written down.
//!
//! `cinder-home/deploy/components.conf` is the single source of truth — it is embedded at build
//! time (see `build.rs`) rather than duplicated here, so the picker can never offer a component
//! the device-side `install_cinderhome.sh` does not understand, and vice versa.

/// What values a component accepts. `Bool` is a checkbox, `Enum` a one-of-N choice.
#[derive(Clone, PartialEq, Eq)]
pub enum Kind {
    Bool,
    Enum(Vec<String>),
}

/// One value of an enum component as the person installing sees it: `label` is what the
/// drop-down shows, `help` the sentence that says what picking it does.
#[derive(Clone)]
pub struct Choice {
    pub value: String,
    pub label: String,
    pub help: String,
}

#[derive(Clone)]
pub struct Comp {
    pub id: String,
    pub var: String,
    pub kind: Kind,
    pub default: String,
    pub title: String,
    /// Paragraphs separated by a blank line; a bullet is a line of its own. Lines are NOT
    /// wrapped — each front end wraps to its own width.
    pub desc: String,
    /// One per enum value, in the catalogue's order. Empty for a bool.
    pub choices: Vec<Choice>,
    pub value: String,
}

impl Comp {
    pub fn allowed(&self) -> Vec<String> {
        match &self.kind {
            Kind::Bool => vec!["0".into(), "1".into()],
            Kind::Enum(v) => v.clone(),
        }
    }

    pub fn valid(&self, v: &str) -> bool {
        self.allowed().iter().any(|a| a == v)
    }

    pub fn cycle(&mut self) {
        let a = self.allowed();
        let i = a.iter().position(|x| *x == self.value).unwrap_or(0);
        self.value = a[(i + 1) % a.len()].clone();
    }

    pub fn is_on(&self) -> bool {
        self.value != "0"
    }

    /// A value as the person installing reads it: "on"/"off" for a bool, the choice's label for
    /// an enum, with "(default)" after the catalogue's default.
    pub fn label_of(&self, v: &str) -> String {
        let base = match self.kind {
            Kind::Bool => if v == "0" { "off" } else { "on" }.to_string(),
            Kind::Enum(_) => self
                .choices
                .iter()
                .find(|c| c.value == v)
                .map_or_else(|| v.to_string(), |c| c.label.clone()),
        };
        if v == self.default && matches!(self.kind, Kind::Enum(_)) {
            format!("{base} (default)")
        } else {
            base
        }
    }

    /// The current value, as `label_of` puts it.
    pub fn shown(&self) -> String {
        self.label_of(&self.value)
    }

    /// The description laid out for reading: the first paragraph, then every choice with the
    /// current one marked `on` and the rest `off`, then the remaining paragraphs. The choices go
    /// after the first paragraph because that paragraph says what is being chosen.
    pub fn explain(&self, on: &str, off: &str) -> String {
        let (first, rest) = match self.desc.split_once("\n\n") {
            Some((a, b)) => (a, Some(b)),
            None => (self.desc.as_str(), None),
        };
        let mut s = first.to_string();
        if !self.choices.is_empty() {
            s.push_str("\n\n");
            for (n, c) in self.choices.iter().enumerate() {
                if n > 0 {
                    s.push('\n');
                }
                let mark = if c.value == self.value { on } else { off };
                s.push_str(&format!("{mark} {} — {}", self.label_of(&c.value), c.help));
            }
        }
        if let Some(r) = rest {
            s.push_str("\n\n");
            s.push_str(r);
        }
        s
    }
}

/// Parse `deploy/components.conf`. Format: `id | VARNAME | type | default | title`, followed by
/// indented lines: description text (a blank line starts a paragraph, `- ` a bullet), one
/// `= value | label | help` line per enum value, and `#` notes that are never shown. The header
/// of `components.conf` is the full grammar; `tools/configure.sh` reads the same one.
pub fn parse_catalogue(text: &str) -> Result<Vec<Comp>, String> {
    let mut out: Vec<Comp> = Vec::new();
    // a blank line was seen since the last description line, so the next one opens a paragraph
    let mut para = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('#') {
            continue;
        }
        if t.is_empty() {
            para = true;
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
                choices: Vec::new(),
                value: f[3].into(),
            });
            para = false;
        } else if indented {
            let Some(last) = out.last_mut() else { continue };
            if let Some(rest) = t.strip_prefix('=') {
                let f: Vec<&str> = rest.splitn(3, '|').map(str::trim).collect();
                if f.len() < 3 || f.iter().any(|x| x.is_empty()) {
                    return Err(format!("malformed choice line under '{}': {t}", last.id));
                }
                last.choices.push(Choice { value: f[0].into(), label: f[1].into(), help: f[2].into() });
                continue;
            }
            if !last.desc.is_empty() {
                last.desc.push_str(if para {
                    "\n\n"
                } else if t.starts_with("- ") {
                    "\n"
                } else {
                    " "
                });
            }
            last.desc.push_str(t);
            para = false;
        }
    }
    if out.is_empty() {
        return Err("catalogue contained no components".into());
    }
    for c in &out {
        if !c.valid(&c.default) {
            return Err(format!("default '{}' invalid for '{}'", c.default, c.id));
        }
        // Every value the drop-down can hold gets a label and a sentence, and nothing else does.
        // A bare `wm1a` in a picker is the thing this format exists to stop.
        match &c.kind {
            Kind::Bool if !c.choices.is_empty() => {
                return Err(format!("'{}' is a bool and cannot have choice lines", c.id));
            }
            Kind::Enum(vals) => {
                let named: Vec<&str> = c.choices.iter().map(|x| x.value.as_str()).collect();
                if named != vals.iter().map(String::as_str).collect::<Vec<_>>() {
                    return Err(format!(
                        "'{}' needs one '= value | label | help' line per value, in the order {}",
                        c.id,
                        vals.join(",")
                    ));
                }
            }
            Kind::Bool => {}
        }
    }
    Ok(out)
}

/// The file the device reads back. Every non-comment line is a bare `KEY=VALUE`, because the
/// device side greps for exactly that shape and ignores anything else.
pub fn conf_text(comps: &[Comp], channel: &str) -> String {
    let mut s = String::new();
    s.push_str("# cinder_components.conf - generated by cinder-installer; do not edit by hand.\n");
    s.push_str("# Read (never sourced) by install_cinderhome.sh on the device.\n");
    s.push_str(&format!("# channel: {channel}\n"));
    s.push_str(&format!("# installer: {}\n", crate::VERSION));
    for c in comps {
        s.push_str(&format!("\n# {}\n{}={}\n", c.title, c.var, c.value));
    }
    s
}

/// Re-apply the choices recorded in a `cinder_components.conf` already on the player.
///
/// This is what makes UPDATE different from INSTALL: an update must not silently reset a user
/// back to catalogue defaults, and the answers are already sitting on the drive from last time.
/// Unknown keys and values the current catalogue rejects are ignored rather than fatal — the
/// file may have been written by an older installer whose catalogue differed.
///
/// Returns the number of components it actually set.
pub fn apply_saved(comps: &mut [Comp], text: &str) -> usize {
    let mut n = 0;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let (k, v) = (k.trim(), v.trim());
        if let Some(c) = comps.iter_mut().find(|c| c.var == k) {
            if c.valid(v) {
                c.value = v.to_string();
                n += 1;
            }
        }
    }
    n
}

/// The `# channel:` / `# installer:` breadcrumbs a previous run left in the conf. Either may be
/// absent — installers before this one wrote only the channel.
pub fn saved_stamp(text: &str) -> (Option<String>, Option<String>) {
    let get = |key: &str| {
        text.lines()
            .map(str::trim)
            .find_map(|l| l.strip_prefix(key).map(|v| v.trim().to_string()))
            .filter(|v| !v.is_empty())
    };
    (get("# channel:"), get("# installer:"))
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
    = stock | Sony standard | No change.
    = pv1   | Plus v1       | Walkman One's first.
    = pv2   | Plus v2       | Walkman One's second.

    Second paragraph.
    # a developer note, never shown
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
        assert_eq!(c[0].shown(), "on");
        c[0].cycle();
        assert_eq!(c[0].value, "0");
        assert_eq!(c[0].shown(), "off");
    }

    // ── what the person installing reads ───────────────────────────────────────────────────

    #[test]
    fn lines_join_paragraphs_split_and_notes_vanish() {
        let c = parse_catalogue(SAMPLE).unwrap();
        assert_eq!(c[0].desc, "Installs cinder-power. Second line.", "wrapped lines join with a space");
        assert_eq!(c[1].desc, "Patches three bytes.\n\nSecond paragraph.");
        assert!(!c[1].desc.contains("developer note"));
        let b = parse_catalogue("a | A | bool | 1 | T\n    One.\n    - first\n    - second\n").unwrap();
        assert_eq!(b[0].desc, "One.\n- first\n- second", "a bullet starts its own line");
    }

    #[test]
    fn choices_are_labelled_and_the_current_one_is_marked() {
        let mut c = parse_catalogue(SAMPLE).unwrap();
        let sig = &mut c[1];
        assert_eq!(sig.shown(), "Sony standard (default)");
        sig.value = "pv2".into();
        assert_eq!(sig.shown(), "Plus v2");
        assert_eq!(
            sig.explain("*", "-"),
            "Patches three bytes.\n\n\
             - Sony standard (default) — No change.\n\
             - Plus v1 — Walkman One's first.\n\
             * Plus v2 — Walkman One's second.\n\n\
             Second paragraph."
        );
        assert_eq!(c[0].explain("*", "-"), c[0].desc, "a bool has no choice list");
    }

    #[test]
    fn every_enum_value_needs_exactly_one_choice_line() {
        let head = "s | S | enum:a,b | a | T\n    Text.\n";
        assert!(parse_catalogue(&format!("{head}    = a | A | x\n")).is_err(), "b has no label");
        assert!(parse_catalogue(&format!("{head}    = a | A | x\n    = c | C | x\n")).is_err(), "c is not a value");
        assert!(parse_catalogue(&format!("{head}    = a | A\n    = b | B | x\n")).is_err(), "a has no help");
        assert!(parse_catalogue(&format!("{head}    = a | A | x\n    = b | B | y\n")).is_ok());
        assert!(parse_catalogue("p | P | bool | 1 | T\n    = 1 | On | x\n").is_err(), "not on a bool");
    }

    /// The catalogue this installer actually embeds. Its text is written for the person
    /// installing, and these words are the RE notes it used to be made of (2026-10-05: the
    /// owner called them "terrible" and "overly complicated"). They belong in the `#` notes.
    #[test]
    fn the_real_catalogue_speaks_plainly() {
        let comps = parse_catalogue(crate::CATALOGUE.expect("components.conf is embedded")).unwrap();
        for c in &comps {
            let shown = format!("{} {}", c.title, c.explain("*", "-"));
            for jargon in ["/proc", "/data/", "/system", "hw:0", "setuid", "SHA-256", "libaudiohal", ".so ", "I2C", "ALSA", "regmon"] {
                assert!(!shown.contains(jargon), "'{}' shows '{jargon}' to the person installing", c.id);
            }
            let words = shown.split_whitespace().count();
            assert!(words <= 180, "'{}' is {words} words — say less, put the rest in a # note", c.id);
        }
    }

    #[test]
    fn generated_conf_is_parseable_key_values() {
        let c = parse_catalogue(SAMPLE).unwrap();
        let t = conf_text(&c, "stable");
        assert!(t.contains("CINDER_POWER=1"));
        assert!(t.contains("CINDER_SIGNATURE=stock"));
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

    // ── update: re-reading the answers already on the player ───────────────────────────────

    #[test]
    fn saved_answers_come_back() {
        let mut c = parse_catalogue(SAMPLE).unwrap();
        let saved = "# channel: dev\nCINDER_POWER=0\nCINDER_SIGNATURE=pv2\n";
        assert_eq!(apply_saved(&mut c, saved), 2);
        assert_eq!(c[0].value, "0");
        assert_eq!(c[1].value, "pv2");
    }

    /// An update must round-trip: what one run writes, the next run must read back unchanged.
    /// This is the whole contract that lets "Update" keep a user's choices.
    #[test]
    fn a_written_conf_reloads_to_the_same_answers() {
        let mut a = parse_catalogue(SAMPLE).unwrap();
        a[0].value = "0".into();
        a[1].value = "pv1".into();
        let text = conf_text(&a, "stable");

        let mut b = parse_catalogue(SAMPLE).unwrap();
        assert_eq!(apply_saved(&mut b, &text), 2);
        for (x, y) in a.iter().zip(b.iter()) {
            assert_eq!(x.value, y.value, "{} did not round-trip", x.id);
        }
    }

    /// A conf written by an older installer can name components this build has dropped, or carry
    /// a value the catalogue no longer allows. Neither is a reason to refuse to update.
    #[test]
    fn unknown_keys_and_stale_values_are_skipped_not_fatal() {
        let mut c = parse_catalogue(SAMPLE).unwrap();
        let saved = "CINDER_GONE=1\nCINDER_SIGNATURE=pv9\nCINDER_POWER=0\nrubbish\n";
        assert_eq!(apply_saved(&mut c, saved), 1, "only CINDER_POWER is applicable");
        assert_eq!(c[0].value, "0");
        assert_eq!(c[1].value, "stock", "an invalid value leaves the default alone");
    }

    #[test]
    fn the_stamp_is_read_back_when_present() {
        let c = parse_catalogue(SAMPLE).unwrap();
        let (ch, ver) = saved_stamp(&conf_text(&c, "dev"));
        assert_eq!(ch.as_deref(), Some("dev"));
        assert_eq!(ver.as_deref(), Some(crate::VERSION));

        let (ch, ver) = saved_stamp("CINDER_POWER=1\n");
        assert_eq!(ch, None);
        assert_eq!(ver, None, "an older conf has no installer stamp and that is not an error");
    }
}
