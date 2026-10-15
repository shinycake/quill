use super::language_rows;
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
    let (top, rest) = language_rows(&list, &recent, "x-pirate", "");
    // Official ids never repeat in the recent section; the current is first.
    let ids: Vec<&str> = top.iter().map(|r| r.info.id.as_str()).collect();
    assert_eq!(ids, ["x-pirate", "x-old"]);
    assert_eq!(rest.len(), 2);

    let (top, rest) = language_rows(&list, &recent, "en", "portugues");
    assert!(top.is_empty());
    assert_eq!(rest.len(), 1);
    assert_eq!(rest[0].info.id, "pt-br");

    let (top, rest) = language_rows(&list, &recent, "en", "klingon");
    assert!(top.is_empty() && rest.is_empty());
}
