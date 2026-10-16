use super::*;

fn pack(strings: &[(&str, PackValue)]) -> Snapshot {
    let mut snapshot = Snapshot::new("pt-br", "pt", false);
    snapshot.layer(strings.iter().map(|(k, v)| (*k, v)));
    snapshot
}

fn plural(one: &str, other: &str) -> PackValue {
    PackValue::Pluralized([
        String::new(),
        one.into(),
        String::new(),
        String::new(),
        String::new(),
        other.into(),
    ])
}

#[test]
fn english_is_the_fallback() {
    assert_eq!(text_in(None, keys::LANGUAGES), "Language");
    assert_eq!(english(keys::LANGUAGES), "Language");
    assert_eq!(plural_in(None, keys::LANGUAGES_COUNT, 1, &[]), "1 language");
    assert_eq!(
        plural_in(None, keys::LANGUAGES_COUNT, 3, &[]),
        "3 languages"
    );
    let empty = pack(&[]);
    assert_eq!(text_in(Some(&empty), keys::LANGUAGES), "Language");
}

#[test]
fn pack_values_win_and_unknown_keys_are_ignored() {
    let snapshot = pack(&[
        ("lng_languages", PackValue::Ordinary("Idioma".into())),
        (
            "lng_languages_count",
            plural("{count} idioma", "{count} idiomas"),
        ),
        ("lng_not_used_by_quill", PackValue::Ordinary("x".into())),
    ]);
    assert_eq!(text_in(Some(&snapshot), keys::LANGUAGES), "Idioma");
    // Portuguese: 0 and 1 are "one".
    assert_eq!(
        plural_in(Some(&snapshot), keys::LANGUAGES_COUNT, 0, &[]),
        "0 idioma"
    );
    assert_eq!(
        plural_in(Some(&snapshot), keys::LANGUAGES_COUNT, 2, &[]),
        "2 idiomas"
    );
}

#[test]
fn missing_form_falls_back_to_other_and_kind_mismatch_to_english() {
    let snapshot = pack(&[
        ("lng_languages_count", plural("", "{count} idiomas")),
        ("lng_languages", plural("a", "b")),
    ]);
    assert_eq!(
        plural_in(Some(&snapshot), keys::LANGUAGES_COUNT, 1, &[]),
        "1 idiomas"
    );
    assert_eq!(text_in(Some(&snapshot), keys::LANGUAGES), "Language");
}

#[test]
fn patch_changes_and_deletes() {
    let snapshot = pack(&[
        ("lng_languages", PackValue::Ordinary("Idioma".into())),
        ("lng_box_ok", PackValue::Ordinary("OK!".into())),
    ]);
    let next = snapshot.patched([
        ("lng_languages", &PackValue::Ordinary("Língua".into())),
        ("lng_box_ok", &PackValue::Deleted),
    ]);
    assert_eq!(text_in(Some(&next), keys::LANGUAGES), "Língua");
    assert_eq!(text_in(Some(&next), keys::BOX_OK), "OK");
    // The original is untouched (copy-on-write).
    assert_eq!(text_in(Some(&snapshot), keys::BOX_OK), "OK!");
}

#[test]
fn tags_and_counts() {
    assert_eq!(
        format::substitute("{a} and {b}", &[("a", "x")]),
        "x and {b}"
    );
    assert_eq!(format::substitute("no tags", &[]), "no tags");
    assert_eq!(format::substitute("open {", &[]), "open {");
    assert_eq!(format::group_digits(1234567), "1,234,567");
    assert_eq!(format::group_digits(-1000), "-1,000");
    assert_eq!(format::group_digits(999), "999");
    assert_eq!(format::short_count(9999), "9999");
    assert_eq!(format::short_count(12345), "12.3K");
    assert_eq!(format::short_count(2_000_000), "2M");
    assert_eq!(format::short_count(3_100_000_000), "3.1B");
}

#[test]
fn entries_round_trip_through_a_snapshot() {
    let snapshot = pack(&[
        ("lng_languages", PackValue::Ordinary("Idioma".into())),
        (
            "lng_languages_count",
            plural("{count} idioma", "{count} idiomas"),
        ),
    ]);
    let entries = snapshot.entries();
    assert_eq!(entries.len(), 2);
    let rebuilt = pack(
        &entries
            .iter()
            .map(|(k, v)| (k.as_str(), v.clone()))
            .collect::<Vec<_>>(),
    );
    assert_eq!(rebuilt.entries(), entries);
    assert_eq!(snapshot.translated_count("lng_").0, 2);
}

#[test]
fn english_strings_use_the_ellipsis_character() {
    for (name, text) in [
        ("av", include_str!("../../lang/en/av.strings")),
        ("chat", include_str!("../../lang/en/chat.strings")),
        ("media", include_str!("../../lang/en/media.strings")),
        ("money", include_str!("../../lang/en/money.strings")),
        ("ops", include_str!("../../lang/en/ops.strings")),
        ("people", include_str!("../../lang/en/people.strings")),
        ("prefs", include_str!("../../lang/en/prefs.strings")),
        ("search", include_str!("../../lang/en/search.strings")),
        (
            "stories_live",
            include_str!("../../lang/en/stories_live.strings"),
        ),
    ] {
        assert!(
            !text.contains("..."),
            "{name}.strings has a three-dot ellipsis"
        );
    }
}
