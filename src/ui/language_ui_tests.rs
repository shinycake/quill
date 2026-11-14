use super::{LanguageLists, language_rows};
use quill::lang::prefs::LanguageChoice;
use quill::telegram::envelope::LanguagePackInfoData;

fn official(id: &str, name: &str, native: &str) -> LanguagePackInfoData {
    LanguagePackInfoData {
        id: id.into(),
        base_language_pack_id: String::new(),
        name: name.into(),
        native_name: native.into(),
        plural_code: id.into(),
        is_official: true,
        is_rtl: false,
        is_beta: false,
        is_installed: false,
        total_string_count: 0,
        translated_string_count: 0,
    }
}

fn custom(id: &str, name: &str) -> LanguageChoice {
    LanguageChoice {
        pack_id: id.into(),
        name: name.into(),
        native_name: name.into(),
        ..Default::default()
    }
}

#[test]
fn recent_unofficial_packs_come_first_and_search_folds_diacritics() {
    let list = [
        official("en", "English", "English"),
        official("pt-br", "Portuguese (Brazil)", "Português (Brasil)"),
    ];
    let recent = [
        custom("x-old", "Old Custom"),
        custom("x-pirate", "Pirate"),
        custom("pt-br", "dup"),
    ];
    let LanguageLists {
        recent: top,
        official: rest,
        ..
    } = language_rows(&list, &[], &recent, "x-pirate", "");
    // Official ids never repeat in the recent section; the current is first.
    let ids: Vec<&str> = top.iter().map(|r| r.info.id.as_str()).collect();
    assert_eq!(ids, ["x-pirate", "x-old"]);
    assert_eq!(rest.len(), 2);

    let LanguageLists {
        recent: top,
        official: rest,
        ..
    } = language_rows(&list, &[], &recent, "en", "portugues");
    assert!(top.is_empty());
    assert_eq!(rest.len(), 1);
    assert_eq!(rest[0].info.id, "pt-br");

    let LanguageLists {
        recent: top,
        official: rest,
        ..
    } = language_rows(&list, &[], &recent, "en", "klingon");
    assert!(top.is_empty() && rest.is_empty());
}

#[test]
fn beta_packs_list_after_the_official_ones_unless_already_shown() {
    let list = [official("en", "English", "English")];
    let beta = |id: &str, name: &str, native: &str| LanguagePackInfoData {
        is_official: false,
        is_beta: true,
        ..official(id, name, native)
    };
    let betas = [
        beta("vi-raw", "Vietnamese", "Tiếng Việt (beta)"),
        beta("ja-raw", "Japanese", "日本語 (beta)"),
    ];
    // A beta pack used before shows once, in the recent section.
    let recent = [custom("ja-raw", "Japanese")];
    let lists = language_rows(&list, &betas, &recent, "en", "");
    let ids = |rows: &[super::LanguageRow]| -> Vec<String> {
        rows.iter().map(|r| r.info.id.clone()).collect()
    };
    assert_eq!(ids(&lists.recent), ["ja-raw"]);
    assert_eq!(ids(&lists.official), ["en"]);
    assert_eq!(ids(&lists.beta), ["vi-raw"]);

    // Search finds a beta pack by its native name, diacritics folded.
    let lists = language_rows(&list, &betas, &[], "en", "tieng viet");
    assert!(lists.official.is_empty());
    assert_eq!(ids(&lists.beta), ["vi-raw"]);
}

#[test]
fn beta_names_drop_the_suffix_telegram_adds() {
    assert_eq!(super::shown_name("Tiếng Việt (beta)"), "Tiếng Việt");
    assert_eq!(super::shown_name("Русский (raw)"), "Русский");
    assert_eq!(super::shown_name("Deutsch"), "Deutsch");
}

#[test]
fn kit_strings_follow_the_pack_where_the_kit_has_the_language() {
    for (pack, language, locale) in [
        ("en", "en", "en"),
        ("ru", "ru", "ru"),
        ("ru-raw", "ru", "ru"),
        ("my-custom-pack", "ru", "ru"),
        ("pt-br", "pt-br", "pt-BR"),
        ("classic-zh-cn", "zh", "zh-CN"),
        ("zhcncc", "zh", "zh-CN"),
        ("zh-hant-raw", "zh", "zh-TW"),
        ("zh-hant-hk", "zh", "zh-HK"),
        ("he", "he", "en"),
    ] {
        assert_eq!(super::kit_locale(pack, language), locale, "{pack}");
    }
}
