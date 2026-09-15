use std::fs;
use tempfile::tempdir;

use reposhelf_lib::error::AppError;
use reposhelf_lib::settings::{
    detect_and_resolve_windows_locale, map_language_tag_to_locale,
    map_preferred_languages_to_locale, SettingsStore, UiLocale, WindowsUiLanguageDetector,
};

struct MockDetector {
    result: Result<Vec<String>, String>,
}

impl MockDetector {
    fn with_languages(langs: Vec<&str>) -> Self {
        Self {
            result: Ok(langs.into_iter().map(|s| s.to_string()).collect()),
        }
    }

    fn failing() -> Self {
        Self {
            result: Err("Mock detector API failure".to_string()),
        }
    }
}

impl WindowsUiLanguageDetector for MockDetector {
    fn detect_user_preferred_ui_languages(&self) -> Result<Vec<String>, String> {
        self.result.clone()
    }
}

// ===========================================================================
// SECTION 30: REQUIRED LOCALE-MAPPING TEST MATRIX
// ===========================================================================

#[test]
fn test_matrix_pure_mapping_rules() {
    // en variants => en
    assert_eq!(map_language_tag_to_locale("en"), UiLocale::En);
    assert_eq!(map_language_tag_to_locale("en-US"), UiLocale::En);
    assert_eq!(map_language_tag_to_locale("en-GB"), UiLocale::En);
    assert_eq!(map_language_tag_to_locale("en-AU"), UiLocale::En);

    // explicit simplified script => zh-Hans
    assert_eq!(map_language_tag_to_locale("zh-Hans"), UiLocale::ZhHans);
    assert_eq!(map_language_tag_to_locale("zh-Hans-CN"), UiLocale::ZhHans);
    assert_eq!(map_language_tag_to_locale("zh-Hans-TW"), UiLocale::ZhHans);

    // explicit traditional script => zh-Hant
    assert_eq!(map_language_tag_to_locale("zh-Hant"), UiLocale::ZhHant);
    assert_eq!(map_language_tag_to_locale("zh-Hant-TW"), UiLocale::ZhHant);
    assert_eq!(map_language_tag_to_locale("zh-Hant-CN"), UiLocale::ZhHant);

    // Chinese without explicit script:
    assert_eq!(map_language_tag_to_locale("zh-CN"), UiLocale::ZhHans);
    assert_eq!(map_language_tag_to_locale("zh-SG"), UiLocale::ZhHans);
    assert_eq!(map_language_tag_to_locale("zh-TW"), UiLocale::ZhHant);
    assert_eq!(map_language_tag_to_locale("zh-HK"), UiLocale::ZhHant);
    assert_eq!(map_language_tag_to_locale("zh-MO"), UiLocale::ZhHant);

    // Ambiguous zh => en
    assert_eq!(map_language_tag_to_locale("zh"), UiLocale::En);

    // Unsupported languages => en
    assert_eq!(map_language_tag_to_locale("fr-FR"), UiLocale::En);
    assert_eq!(map_language_tag_to_locale("ja-JP"), UiLocale::En);
    assert_eq!(map_language_tag_to_locale("de-DE"), UiLocale::En);

    // Malformed / invalid => en
    assert_eq!(map_language_tag_to_locale("invalid-1234"), UiLocale::En);
    assert_eq!(map_language_tag_to_locale(""), UiLocale::En);

    // Preferred language list semantics:
    // first = fr-FR, second = zh-CN => result MUST equal en (do not scan later)
    let list = vec!["fr-FR".to_string(), "zh-CN".to_string()];
    assert_eq!(map_preferred_languages_to_locale(&list), UiLocale::En);

    // empty list / no result => en
    assert_eq!(map_preferred_languages_to_locale(&[]), UiLocale::En);

    // detector failure => en
    let (res, _) = detect_and_resolve_windows_locale(&MockDetector::failing());
    assert_eq!(res, UiLocale::En);
}

// ===========================================================================
// SECTION 31: REQUIRED SETTINGS INITIALIZATION TEST MATRIX
// ===========================================================================

// TEST 1 — Existing en
#[test]
fn test_s4_init_test1_existing_en() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("settings.json");

    // Write valid settings with ui_language = "en"
    let initial_json = r#"{
        "schema_version": 1,
        "github": { "default_search_limit": 42, "default_trending_period": "daily" },
        "ai": { "enabled": false, "provider": null, "base_url": null, "model": null },
        "ui_language": "en"
    }"#;
    fs::write(&config_path, initial_json).unwrap();

    let detector = MockDetector::with_languages(vec!["zh-CN"]); // Detector would return zh-CN if called
    let store = SettingsStore::new_with_detector(config_path.clone(), &detector).unwrap();

    let loaded = store.get().unwrap();
    // Expected: use en, detector not invoked (or won't overwrite), settings not rewritten
    assert_eq!(loaded.ui_language, Some(UiLocale::En));
    assert_eq!(loaded.github.default_search_limit, 42);
    assert_eq!(store.get_detected_source_tag(), None);
}

// TEST 2 — Existing zh-Hans
#[test]
fn test_s4_init_test2_existing_zh_hans() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("settings.json");

    let initial_json = r#"{
        "schema_version": 1,
        "github": { "default_search_limit": 30, "default_trending_period": "weekly" },
        "ai": { "enabled": false, "provider": null, "base_url": null, "model": null },
        "ui_language": "zh-Hans"
    }"#;
    fs::write(&config_path, initial_json).unwrap();

    let detector = MockDetector::with_languages(vec!["en-US"]);
    let store = SettingsStore::new_with_detector(config_path.clone(), &detector).unwrap();

    let loaded = store.get().unwrap();
    assert_eq!(loaded.ui_language, Some(UiLocale::ZhHans));
    assert_eq!(loaded.github.default_search_limit, 30);
    assert_eq!(store.get_detected_source_tag(), None);
}

// TEST 3 — Existing zh-Hant
#[test]
fn test_s4_init_test3_existing_zh_hant() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("settings.json");

    let initial_json = r#"{
        "schema_version": 1,
        "github": { "default_search_limit": 15, "default_trending_period": "monthly" },
        "ai": { "enabled": false, "provider": null, "base_url": null, "model": null },
        "ui_language": "zh-Hant"
    }"#;
    fs::write(&config_path, initial_json).unwrap();

    let detector = MockDetector::with_languages(vec!["en-US"]);
    let store = SettingsStore::new_with_detector(config_path.clone(), &detector).unwrap();

    let loaded = store.get().unwrap();
    assert_eq!(loaded.ui_language, Some(UiLocale::ZhHant));
    assert_eq!(loaded.github.default_search_limit, 15);
    assert_eq!(store.get_detected_source_tag(), None);
}

// TEST 4 — Safely loaded settings, missing ui_language
#[test]
fn test_s4_init_test4_missing_ui_language() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("settings.json");

    // v1.0.0 settings without ui_language
    let v1_json = r#"{
        "schema_version": 1,
        "github": { "default_search_limit": 25, "default_trending_period": "daily" },
        "ai": { "enabled": true, "provider": "ollama", "base_url": "http://localhost:11434", "model": "llama3" }
    }"#;
    fs::write(&config_path, v1_json).unwrap();

    let detector = MockDetector::with_languages(vec!["zh-CN", "en-US"]);
    let store = SettingsStore::new_with_detector(config_path.clone(), &detector).unwrap();

    let loaded = store.get().unwrap();
    // Detector invoked once, mapped to zh-Hans, only language added, unrelated settings preserved
    assert_eq!(loaded.ui_language, Some(UiLocale::ZhHans));
    assert_eq!(loaded.github.default_search_limit, 25);
    assert!(loaded.ai.enabled);
    assert_eq!(loaded.ai.model.as_deref(), Some("llama3"));

    // Verify on-disk file was updated to include ui_language
    let on_disk_str = fs::read_to_string(&config_path).unwrap();
    assert!(on_disk_str.contains(r#""ui_language": "zh-Hans""#));
}

// TEST 5 — Safely loaded settings, invalid ui_language
#[test]
fn test_s4_init_test5_invalid_ui_language() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("settings.json");

    let invalid_json = r#"{
        "schema_version": 1,
        "github": { "default_search_limit": 33, "default_trending_period": "daily" },
        "ai": { "enabled": false, "provider": null, "base_url": null, "model": null },
        "ui_language": "fr-FR"
    }"#;
    fs::write(&config_path, invalid_json).unwrap();

    let detector = MockDetector::with_languages(vec!["zh-TW"]);
    let store = SettingsStore::new_with_detector(config_path.clone(), &detector).unwrap();

    let loaded = store.get().unwrap();
    // Detector invoked, mapped to zh-Hant, repaired and persisted, unrelated settings preserved
    assert_eq!(loaded.ui_language, Some(UiLocale::ZhHant));
    assert_eq!(loaded.github.default_search_limit, 33);

    let on_disk_str = fs::read_to_string(&config_path).unwrap();
    assert!(on_disk_str.contains(r#""ui_language": "zh-Hant""#));
}

// TEST 6 — Fresh safe settings state without language
#[test]
fn test_s4_init_test6_fresh_settings_state() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("settings.json");
    assert!(!config_path.exists());

    let detector = MockDetector::with_languages(vec!["en-US"]);
    let store = SettingsStore::new_with_detector(config_path.clone(), &detector).unwrap();

    let loaded = store.get().unwrap();
    assert_eq!(loaded.ui_language, Some(UiLocale::En));
    assert!(config_path.exists());

    let on_disk_str = fs::read_to_string(&config_path).unwrap();
    assert!(on_disk_str.contains(r#""ui_language": "en""#));
}

// TEST 7 — Corrupt settings/read failure
#[test]
fn test_s4_init_test7_corrupt_settings_read_failure() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("settings.json");
    fs::write(&config_path, "{ corrupt json ...").unwrap();

    let detector = MockDetector::with_languages(vec!["zh-CN"]);
    let result = SettingsStore::new_with_detector(config_path.clone(), &detector);

    // Expected: NOT treated as missing key, does not overwrite file, returns SettingsRead error
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), AppError::SettingsRead(_)));

    let raw_content = fs::read_to_string(&config_path).unwrap();
    assert_eq!(raw_content, "{ corrupt json ...");
}

// TEST 8 — Permission/read failure (simulated via invalid path or directory as file)
#[test]
fn test_s4_init_test8_read_failure_non_destructive() {
    let dir = tempdir().unwrap();
    let config_dir_as_file = dir.path().join("settings.json");
    fs::create_dir(&config_dir_as_file).unwrap(); // Directory cannot be read as file

    let detector = MockDetector::with_languages(vec!["en-US"]);
    let result = SettingsStore::new_with_detector(config_dir_as_file.clone(), &detector);

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), AppError::SettingsRead(_)));
}

// TEST 9 — Windows detector failure
#[test]
fn test_s4_init_test9_detector_failure_resolves_to_en() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("settings.json");

    let detector = MockDetector::failing();
    let store = SettingsStore::new_with_detector(config_path.clone(), &detector).unwrap();

    let loaded = store.get().unwrap();
    // Resolves to en
    assert_eq!(loaded.ui_language, Some(UiLocale::En));
    let on_disk_str = fs::read_to_string(&config_path).unwrap();
    assert!(on_disk_str.contains(r#""ui_language": "en""#));
}

// TEST 10 — Persistence failure
#[test]
fn test_s4_init_test10_persistence_failure_retains_in_memory_locale() {
    let dir = tempdir().unwrap();
    // Simulate persistence failure using a blocker file preventing directory creation
    // In our implementation atomic_write calls create_dir_all, so let's make a read-only file blocking the dir.
    let blocker = dir.path().join("blocker");
    fs::write(&blocker, "blocker").unwrap();
    let bad_config_path = blocker.join("settings.json"); // Cannot create dir inside file

    let detector = MockDetector::with_languages(vec!["zh-CN"]);
    let store = SettingsStore::new_with_detector(bad_config_path, &detector).unwrap();

    // Store is initialized with in-memory resolved locale despite write failure
    let loaded = store.get().unwrap();
    assert_eq!(loaded.ui_language, Some(UiLocale::ZhHans));
    // Warning is recorded indicating write failure
    assert!(store.get_initial_warning().is_some());
}

// Single-field language update test
#[test]
fn test_s4_single_field_language_update() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("settings.json");

    let detector = MockDetector::with_languages(vec!["en-US"]);
    let store = SettingsStore::new_with_detector(config_path.clone(), &detector).unwrap();

    assert_eq!(store.get().unwrap().ui_language, Some(UiLocale::En));

    // Update to zh-Hans
    let updated = store.update_ui_language(UiLocale::ZhHans).unwrap();
    assert_eq!(updated.ui_language, Some(UiLocale::ZhHans));
    assert_eq!(store.get().unwrap().ui_language, Some(UiLocale::ZhHans));

    // Reload from disk and verify
    let reloaded = SettingsStore::new_with_detector(config_path, &detector).unwrap();
    assert_eq!(reloaded.get().unwrap().ui_language, Some(UiLocale::ZhHans));
}

#[test]
#[cfg(target_os = "windows")]
fn test_live_windows_detection() {
    let detector = reposhelf_lib::settings::SystemWindowsUiLanguageDetector;
    let (locale, tag) = detect_and_resolve_windows_locale(&detector);
    println!("Live Windows detected locale: {:?}, tag: {:?}", locale, tag);
    assert!(matches!(
        locale,
        UiLocale::En | UiLocale::ZhHans | UiLocale::ZhHant
    ));
}
