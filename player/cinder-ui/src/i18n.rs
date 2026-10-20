//! UI-only localization. Domain values and user metadata are never translated implicitly.
//!
//! English source messages are stable catalog keys. The render context is thread-local so a
//! preview or another test cannot change a different renderer's language. A guard restores the
//! previous context, including on early return. Outside rendering, domain models remain English.
use std::cell::Cell;

#[path = "i18n_catalog.rs"]
mod catalog;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Language {
    #[default]
    English,
    SimplifiedChinese,
}

impl Language {
    pub fn from_token(token: &str) -> Self {
        match token {
            "zh-CN" => Self::SimplifiedChinese,
            _ => Self::English,
        }
    }

    pub fn token(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::SimplifiedChinese => "zh-CN",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::SimplifiedChinese => "简体中文",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::English => Self::SimplifiedChinese,
            Self::SimplifiedChinese => Self::English,
        }
    }
}

thread_local! {
    static CURRENT: Cell<Language> = const { Cell::new(Language::English) };
}

#[must_use]
pub struct LanguageGuard(Language, std::marker::PhantomData<std::rc::Rc<()>>);

impl Drop for LanguageGuard {
    fn drop(&mut self) {
        CURRENT.with(|value| value.set(self.0));
    }
}

/// Enter the owning App's language for one render; does not mutate models or global preferences.
pub fn enter(language: Language) -> LanguageGuard {
    LanguageGuard(
        CURRENT.with(|value| value.replace(language)),
        std::marker::PhantomData,
    )
}

pub fn is_chinese() -> bool {
    CURRENT.with(|value| value.get() == Language::SimplifiedChinese)
}

/// Translate an explicitly identified UI message. Unknown messages remain readable as supplied.
pub fn text(source: &str) -> &str {
    if is_chinese() {
        catalog::chinese(source).unwrap_or(source)
    } else {
        source
    }
}

/// Translate a message whose visible English wording is shared by more than one UI meaning.
/// `key` is an explicit semantic catalog identity; English keeps the supplied source wording.
pub fn text_with_key<'a>(source: &'a str, key: &str) -> &'a str {
    if is_chinese() {
        catalog::chinese(key).unwrap_or(source)
    } else {
        source
    }
}

/// Format a known message key with owned, named arguments at the display boundary.
/// Inserted values are appended verbatim and are never parsed as message syntax.
pub fn format_template(source: &str, args: &[(&str, &str)]) -> String {
    let template = if is_chinese() {
        catalog::chinese(source).unwrap_or(source)
    } else {
        source
    };
    let mut output = String::with_capacity(template.len());
    let mut chars = template.chars().peekable();
    let mut positional = 0usize;

    while let Some(ch) = chars.next() {
        match (ch, chars.peek().copied()) {
            ('{', Some('{')) => {
                chars.next();
                output.push('{');
            }
            ('}', Some('}')) => {
                chars.next();
                output.push('}');
            }
            ('{', _) => {
                let mut placeholder = String::new();
                for next in chars.by_ref() {
                    if next == '}' {
                        break;
                    }
                    placeholder.push(next);
                }
                let name = placeholder.split(':').next().unwrap_or(&placeholder).trim();
                let value = if name.is_empty() {
                    let value = args.get(positional).map(|(_, value)| *value);
                    positional += 1;
                    value
                } else {
                    args.iter()
                        .find(|(key, _)| *key == name)
                        .map(|(_, value)| *value)
                };
                if let Some(value) = value {
                    output.push_str(value);
                } else {
                    output.push('{');
                    output.push_str(name);
                    output.push('}');
                }
            }
            _ => output.push(ch),
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contexts_restore_and_unknown_messages_fall_back() {
        assert_eq!(text("Settings"), "Settings");
        {
            let _chinese = enter(Language::SimplifiedChinese);
            assert_eq!(text("Settings"), "设置");
            assert_eq!(text("Untranslated new message"), "Untranslated new message");
            {
                let _english = enter(Language::English);
                assert_eq!(text("Settings"), "Settings");
            }
            assert_eq!(text("Settings"), "设置");
        }
        assert_eq!(text("Settings"), "Settings");
    }

    #[test]
    fn persisted_tokens_are_independent_of_display_names() {
        for language in [Language::English, Language::SimplifiedChinese] {
            assert_eq!(Language::from_token(language.token()), language);
            assert_eq!(language.next().next(), language);
        }
        assert_eq!(Language::from_token("broken"), Language::English);
        assert_eq!(Language::from_token(""), Language::English);
    }

    #[test]
    fn english_only_feature_copy_does_not_require_catalog_changes() {
        let name = "Settings {Off} 中文";
        for language in [Language::English, Language::SimplifiedChinese] {
            let _language = enter(language);
            assert_eq!(text("New feature label"), "New feature label");
            assert_eq!(
                text_with_key("New feature label", "Untranslated feature identity"),
                "New feature label"
            );
            let expected = format!("New feature 3: {name}");
            assert_eq!(
                crate::ui_format!("New feature {count}: {name}", count = 3, name = name),
                expected
            );
            assert_eq!(
                format_template(
                    "New feature {count}: {name}",
                    &[("count", "3"), ("name", name)]
                ),
                expected
            );
        }
    }

    #[test]
    fn a_renderer_cannot_change_another_threads_language() {
        let _chinese = enter(Language::SimplifiedChinese);
        std::thread::spawn(|| assert_eq!(text("Settings"), "Settings"))
            .join()
            .unwrap();
        assert_eq!(text("Settings"), "设置");
    }

    #[test]
    fn saved_playlist_count_is_localized_as_ui_copy() {
        let cases = [
            (
                "Saved to Playlists · 1 song",
                vec![],
                "Saved to Playlists · 1 song",
                "已保存到播放列表 · 1 首曲目",
            ),
            (
                "Saved to Playlists · {n} songs",
                vec![("n", "3")],
                "Saved to Playlists · 3 songs",
                "已保存到播放列表 · 3 首曲目",
            ),
        ];
        for (key, args, english, chinese) in cases {
            assert_eq!(format_template(key, &args), english);
            {
                let _language = enter(Language::SimplifiedChinese);
                assert_eq!(format_template(key, &args), chinese);
            }
            assert_eq!(format_template(key, &args), english);
        }
    }

    #[test]
    fn identified_templates_translate_at_display_and_preserve_inserted_metadata() {
        let name = "Up Next — Settings {Off} 中文";
        let _chinese = enter(Language::SimplifiedChinese);
        assert_eq!(
            format_template("Added to {name}", &[("name", name)]),
            format!("已加入 {name}")
        );
        assert_eq!(
            format_template("Cover: {label}", &[("label", "Settings")]),
            "封面：Settings"
        );
        let special = "Settings | Off — 中文 \"quoted\" {brace}";
        assert_eq!(
            format_template("Added to {name}", &[("name", special)]),
            format!("已加入 {special}")
        );
    }
}
