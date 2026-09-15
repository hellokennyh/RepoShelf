use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UiLocale {
    #[serde(rename = "en")]
    En,
    #[serde(rename = "zh-Hans")]
    ZhHans,
    #[serde(rename = "zh-Hant")]
    ZhHant,
}

impl UiLocale {
    pub fn as_str(&self) -> &'static str {
        match self {
            UiLocale::En => "en",
            UiLocale::ZhHans => "zh-Hans",
            UiLocale::ZhHant => "zh-Hant",
        }
    }

    pub fn from_str_strict(s: &str) -> Option<Self> {
        match s {
            "en" => Some(UiLocale::En),
            "zh-Hans" => Some(UiLocale::ZhHans),
            "zh-Hant" => Some(UiLocale::ZhHant),
            _ => None,
        }
    }
}

impl std::fmt::Display for UiLocale {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Pure, deterministic mapping from language tag to supported UiLocale according to Section 13.
///
/// Rules:
/// - English variants (en, en-US, en-GB, en-AU, normal en-* variants) => en
/// - Explicit Simplified Chinese script (zh-Hans, zh-Hans-*) => zh-Hans
/// - Explicit Traditional Chinese script (zh-Hant, zh-Hant-*) => zh-Hant
/// - Explicit script marker has priority over region:
///     zh-Hant-CN => zh-Hant
///     zh-Hans-TW => zh-Hans
/// - Chinese without explicit script:
///     zh-CN, zh-SG => zh-Hans
///     zh-TW, zh-HK, zh-MO => zh-Hant
/// - Ambiguous "zh" => en
/// - Unsupported languages (fr, fr-FR, ja, ja-JP, de, de-DE, etc.) => en
/// - Malformed / invalid / empty tag => en
pub fn map_language_tag_to_locale(tag: &str) -> UiLocale {
    let tag = tag.trim();
    if tag.is_empty() {
        return UiLocale::En;
    }

    // Split into case-insensitive parts
    let parts: Vec<&str> = tag.split(['-', '_']).collect();
    if parts.is_empty() {
        return UiLocale::En;
    }

    let primary = parts[0];

    // English variants
    if primary.eq_ignore_ascii_case("en") {
        return UiLocale::En;
    }

    // Chinese variants
    if primary.eq_ignore_ascii_case("zh") {
        // Check if there are explicit script markers in subsequent parts
        // Priority: explicit script marker has priority over region
        for part in &parts[1..] {
            if part.eq_ignore_ascii_case("Hans") {
                return UiLocale::ZhHans;
            }
            if part.eq_ignore_ascii_case("Hant") {
                return UiLocale::ZhHant;
            }
        }

        // No explicit script found, check region subtags
        for part in &parts[1..] {
            if part.eq_ignore_ascii_case("CN") || part.eq_ignore_ascii_case("SG") {
                return UiLocale::ZhHans;
            }
            if part.eq_ignore_ascii_case("TW")
                || part.eq_ignore_ascii_case("HK")
                || part.eq_ignore_ascii_case("MO")
            {
                return UiLocale::ZhHant;
            }
        }

        // Ambiguous "zh" or unrecognized Chinese subtag
        return UiLocale::En;
    }

    // All other languages (fr, ja, de, etc.) or malformed tags
    UiLocale::En
}

/// Map a list of preferred language tags according to Section 12 & 13.
/// CRITICAL: Mapping decision is based on the FIRST preferred Windows UI language.
/// Do NOT search later preferred languages for a supported value!
/// E.g. first = fr-FR, second = zh-CN => result MUST be en.
pub fn map_preferred_languages_to_locale(languages: &[String]) -> UiLocale {
    match languages.first() {
        Some(first_lang) => map_language_tag_to_locale(first_lang),
        None => UiLocale::En,
    }
}

/// Trait for detecting Windows user preferred UI language so it can be mocked/injected in tests.
pub trait WindowsUiLanguageDetector: Send + Sync {
    fn detect_user_preferred_ui_languages(&self) -> Result<Vec<String>, String>;
}

/// Production Windows API detector using GetUserPreferredUILanguages.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemWindowsUiLanguageDetector;

impl WindowsUiLanguageDetector for SystemWindowsUiLanguageDetector {
    fn detect_user_preferred_ui_languages(&self) -> Result<Vec<String>, String> {
        #[cfg(target_os = "windows")]
        {
            detect_windows_preferred_ui_languages()
        }
        #[cfg(not(target_os = "windows"))]
        {
            Err("Not running on Windows".to_string())
        }
    }
}

#[cfg(target_os = "windows")]
fn detect_windows_preferred_ui_languages() -> Result<Vec<String>, String> {
    use std::ptr;
    use windows_sys::Win32::Globalization::{GetUserPreferredUILanguages, MUI_LANGUAGE_NAME};

    let mut num_languages: u32 = 0;
    let mut buffer_length: u32 = 0;

    // First call to determine buffer size in WCHARs
    let success = unsafe {
        GetUserPreferredUILanguages(
            MUI_LANGUAGE_NAME,
            &mut num_languages,
            ptr::null_mut(),
            &mut buffer_length,
        )
    };

    if success == 0 || buffer_length == 0 {
        return Err("GetUserPreferredUILanguages failed to get buffer size".to_string());
    }

    let mut buffer = vec![0u16; buffer_length as usize];

    // Second call to retrieve the multi-string (double-null-terminated sequence of null-terminated WCHAR strings)
    let success2 = unsafe {
        GetUserPreferredUILanguages(
            MUI_LANGUAGE_NAME,
            &mut num_languages,
            buffer.as_mut_ptr(),
            &mut buffer_length,
        )
    };

    if success2 == 0 {
        return Err("GetUserPreferredUILanguages failed to retrieve languages".to_string());
    }

    // Parse the multi-string
    let mut languages = Vec::new();
    let mut current = Vec::new();

    for &code_unit in &buffer {
        if code_unit == 0 {
            if !current.is_empty() {
                if let Ok(lang) = String::from_utf16(&current) {
                    languages.push(lang);
                }
                current.clear();
            }
        } else {
            current.push(code_unit);
        }
    }

    Ok(languages)
}

/// Convenience function that detects and resolves the Windows user preferred UI language.
pub fn detect_and_resolve_windows_locale<D: WindowsUiLanguageDetector>(
    detector: &D,
) -> (UiLocale, Option<String>) {
    match detector.detect_user_preferred_ui_languages() {
        Ok(langs) => {
            let first_tag = langs.first().cloned();
            let locale = map_preferred_languages_to_locale(&langs);
            (locale, first_tag)
        }
        Err(_) => (UiLocale::En, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_required_locale_mapping_matrix() {
        // Section 30 requirements:
        // en => en
        assert_eq!(map_language_tag_to_locale("en"), UiLocale::En);
        // en-US => en
        assert_eq!(map_language_tag_to_locale("en-US"), UiLocale::En);
        // en-GB => en
        assert_eq!(map_language_tag_to_locale("en-GB"), UiLocale::En);
        // en-AU => en
        assert_eq!(map_language_tag_to_locale("en-AU"), UiLocale::En);

        // zh-Hans => zh-Hans
        assert_eq!(map_language_tag_to_locale("zh-Hans"), UiLocale::ZhHans);
        // zh-Hans-CN => zh-Hans
        assert_eq!(map_language_tag_to_locale("zh-Hans-CN"), UiLocale::ZhHans);
        // zh-Hans-TW => zh-Hans (explicit script overrides region)
        assert_eq!(map_language_tag_to_locale("zh-Hans-TW"), UiLocale::ZhHans);

        // zh-Hant => zh-Hant
        assert_eq!(map_language_tag_to_locale("zh-Hant"), UiLocale::ZhHant);
        // zh-Hant-TW => zh-Hant
        assert_eq!(map_language_tag_to_locale("zh-Hant-TW"), UiLocale::ZhHant);
        // zh-Hant-CN => zh-Hant (explicit script overrides region)
        assert_eq!(map_language_tag_to_locale("zh-Hant-CN"), UiLocale::ZhHant);

        // Chinese without explicit script:
        // zh-CN => zh-Hans
        assert_eq!(map_language_tag_to_locale("zh-CN"), UiLocale::ZhHans);
        // zh-SG => zh-Hans
        assert_eq!(map_language_tag_to_locale("zh-SG"), UiLocale::ZhHans);
        // zh-TW => zh-Hant
        assert_eq!(map_language_tag_to_locale("zh-TW"), UiLocale::ZhHant);
        // zh-HK => zh-Hant
        assert_eq!(map_language_tag_to_locale("zh-HK"), UiLocale::ZhHant);
        // zh-MO => zh-Hant
        assert_eq!(map_language_tag_to_locale("zh-MO"), UiLocale::ZhHant);

        // Ambiguous zh => en
        assert_eq!(map_language_tag_to_locale("zh"), UiLocale::En);

        // Unsupported languages => en
        assert_eq!(map_language_tag_to_locale("fr-FR"), UiLocale::En);
        assert_eq!(map_language_tag_to_locale("ja-JP"), UiLocale::En);
        assert_eq!(map_language_tag_to_locale("de-DE"), UiLocale::En);
        assert_eq!(map_language_tag_to_locale("es-ES"), UiLocale::En);

        // Malformed / invalid => en
        assert_eq!(map_language_tag_to_locale(""), UiLocale::En);
        assert_eq!(map_language_tag_to_locale("   "), UiLocale::En);
        assert_eq!(map_language_tag_to_locale("!@#$"), UiLocale::En);
        assert_eq!(map_language_tag_to_locale("unknown-tag-xyz"), UiLocale::En);
    }

    #[test]
    fn test_preferred_language_list_semantics() {
        // first = fr-FR, second = zh-CN => result MUST equal en
        let langs = vec!["fr-FR".to_string(), "zh-CN".to_string()];
        assert_eq!(map_preferred_languages_to_locale(&langs), UiLocale::En);

        // empty list => en
        let empty: Vec<String> = vec![];
        assert_eq!(map_preferred_languages_to_locale(&empty), UiLocale::En);

        // first = zh-CN, second = en-US => zh-Hans
        let langs_cn = vec!["zh-CN".to_string(), "en-US".to_string()];
        assert_eq!(
            map_preferred_languages_to_locale(&langs_cn),
            UiLocale::ZhHans
        );
    }
}
