use super::keyword_language_codes;

#[test]
fn typed_script_then_system_language_then_english() {
    assert_eq!(keyword_language_codes("fire", None), ["en"]);
    assert_eq!(
        keyword_language_codes("fire", Some("de_DE.UTF-8")),
        ["de", "en"]
    );
    assert_eq!(
        keyword_language_codes("огонь", Some("en_US.UTF-8")),
        ["ru", "uk", "en"]
    );
    assert_eq!(keyword_language_codes("אש", None), ["he", "en"]);
    assert_eq!(
        keyword_language_codes("火", Some("zh_CN")),
        ["zh", "ja", "en"]
    );
}

#[test]
fn mixed_scripts_ask_for_every_language_once() {
    let codes = keyword_language_codes("fire огонь שלום", Some("ru_RU"));
    assert_eq!(codes, ["ru", "uk", "he", "en"]);
}
