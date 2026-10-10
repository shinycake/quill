use super::{
    TranslatePrefs, bar_label, bar_label_for, choose_translate_to, detect_language, language_name,
    normalize_code, offer_language, search_languages, skip_translate, tracking_enabled,
};

#[test]
fn normalize_maps_regional_and_legacy_tags() {
    assert_eq!(normalize_code("en-US"), Some("en"));
    assert_eq!(normalize_code("zh"), Some("zh-CN"));
    assert_eq!(normalize_code("zh_Hant"), Some("zh-TW"));
    assert_eq!(normalize_code("iw"), Some("he"));
    assert_eq!(normalize_code("pt-BR"), Some("pt"));
    assert_eq!(normalize_code("xx"), None);
}

#[test]
fn names_and_search() {
    assert_eq!(language_name("ru"), "Russian");
    assert_eq!(language_name("xx"), "xx");
    let found = search_languages("рус");
    assert!(found.iter().any(|l| l.0 == "ru"));
    assert!(search_languages("").len() > 100);
    assert_eq!(search_languages("hebrew")[0].0, "he");
}

#[test]
fn detects_by_script() {
    assert_eq!(detect_language("שלום מה שלומך היום"), Some("he"));
    assert_eq!(detect_language("Привет, как дела сегодня?"), Some("ru"));
    assert_eq!(detect_language("Привіт, як справи? Їжак"), Some("uk"));
    assert_eq!(detect_language("مرحبا كيف حالك اليوم"), Some("ar"));
    assert_eq!(detect_language("سلام، حالت چطور است؟ پدر"), Some("fa"));
    assert_eq!(detect_language("こんにちは、元気ですか"), Some("ja"));
    assert_eq!(detect_language("你好，你今天怎么样"), Some("zh-CN"));
    assert_eq!(detect_language("안녕하세요 오늘 어때요"), Some("ko"));
    assert_eq!(detect_language("Γειά σου, τι κάνεις σήμερα"), Some("el"));
    assert_eq!(detect_language("नमस्ते आप कैसे हैं"), Some("hi"));
}

#[test]
fn detects_latin_languages_by_common_words() {
    assert_eq!(
        detect_language("Hello, how are you? I think this is the best one for you."),
        Some("en")
    );
    assert_eq!(
        detect_language("Hola, ¿cómo estás? Esto es muy bueno para todos los que están aquí."),
        Some("es")
    );
    assert_eq!(
        detect_language("Bonjour, je suis très content de vous voir dans cette ville."),
        Some("fr")
    );
    assert_eq!(
        detect_language("Ich habe das nicht gewusst, aber es ist auch ein guter Plan für uns."),
        Some("de")
    );
}

#[test]
fn short_or_letterless_text_is_not_guessed() {
    assert_eq!(detect_language(""), None);
    assert_eq!(detect_language("12345 !!!"), None);
    assert_eq!(detect_language("ok"), None);
    assert_eq!(detect_language("xyz qrs"), None);
}

#[test]
fn prefs_default_to_the_app_language() {
    let prefs = TranslatePrefs::default();
    assert!(prefs.show_button);
    assert!(prefs.translate_chats, "tdesktop default");
    assert_eq!(prefs.to_language("de"), "de");
    assert_eq!(prefs.skip("de"), vec!["de"]);
    assert_eq!(prefs.to_language("xx"), "en");
}

#[test]
fn skip_list_keeps_at_least_one_language() {
    let mut prefs = TranslatePrefs::default();
    assert!(!prefs.toggle_skip("en", "en"), "the last one stays");
    assert!(prefs.toggle_skip("ru", "en"));
    assert_eq!(prefs.skip("en"), vec!["en", "ru"]);
    assert!(prefs.toggle_skip("en", "en"));
    assert_eq!(prefs.skip("en"), vec!["ru"]);
    prefs.add_skip("he", "en");
    prefs.add_skip("he", "en");
    assert_eq!(prefs.skip("en"), vec!["ru", "he"]);
}

#[test]
fn hidden_chats_toggle() {
    let mut prefs = TranslatePrefs::default();
    prefs.set_bar_hidden(7, true);
    prefs.set_bar_hidden(7, true);
    assert!(prefs.bar_hidden(7));
    assert_eq!(prefs.hidden_chats, vec![7]);
    prefs.set_bar_hidden(7, false);
    assert!(!prefs.bar_hidden(7));
}

#[test]
fn prefs_round_trip_and_tolerate_missing_fields() {
    let prefs: TranslatePrefs = serde_json::from_str("{}").unwrap();
    assert_eq!(prefs, TranslatePrefs::default());
    let prefs = TranslatePrefs {
        translate_to: "fr".into(),
        skip_languages: vec!["en".into(), "he".into()],
        ..TranslatePrefs::default()
    };
    let json = serde_json::to_string(&prefs).unwrap();
    assert_eq!(
        serde_json::from_str::<TranslatePrefs>(&json).unwrap(),
        prefs
    );
}

#[test]
fn choose_to_avoids_the_chat_language() {
    assert_eq!(choose_translate_to(Some("ru"), "en", &["en"]), "en");
    assert_eq!(choose_translate_to(Some("en"), "en", &["he", "en"]), "he");
    assert_eq!(choose_translate_to(None, "en", &["en"]), "en");
}

#[test]
fn menu_skips_translate_for_known_or_empty_text() {
    let prefs = TranslatePrefs::default();
    assert!(skip_translate("", &prefs, "en"));
    assert!(skip_translate("1234 5678", &prefs, "en"));
    assert!(skip_translate(
        "Hello, how are you? I think this is the best one for you.",
        &prefs,
        "en"
    ));
    assert!(!skip_translate("Привет, как дела сегодня?", &prefs, "en"));
    // Undetectable text is offered.
    assert!(!skip_translate("xyz qrs", &prefs, "en"));
    let off = TranslatePrefs {
        show_button: false,
        ..TranslatePrefs::default()
    };
    assert!(skip_translate("Привет, как дела сегодня?", &off, "en"));
}

#[test]
fn offers_the_dominant_foreign_language() {
    let ru = "Привет, как дела сегодня?";
    let en = "Hello, how are you? I think this is the best one for you.";
    let few: Vec<&str> = vec![ru; 5];
    assert_eq!(offer_language(few, &["en"]), None, "too few messages");
    let many: Vec<&str> = vec![ru; 8];
    assert_eq!(offer_language(many, &["en"]), Some("ru"));
    let skipped: Vec<&str> = vec![ru; 8];
    assert_eq!(offer_language(skipped, &["ru"]), None);
    let mixed: Vec<&str> = [vec![ru; 3], vec![en; 9]].concat();
    assert_eq!(offer_language(mixed, &["en"]), None, "mostly skipped");
}

#[test]
fn translation_replaces_text_and_captions() {
    use crate::telegram::envelope::{MessageContent, TextContent};
    use crate::text::{TextEntity, TextEntityKind};
    let bold = TextEntity {
        utf8_start: 0,
        utf8_end: 5,
        kind: TextEntityKind::Bold,
    };
    let mut content = MessageContent::Text(TextContent {
        text: "Привет".into(),
        entities: Vec::new(),
        link_preview: None,
    });
    assert_eq!(
        super::translatable_content(&content).map(|(t, _)| t),
        Some("Привет")
    );
    assert!(super::replace_content_text(
        &mut content,
        "Hello world",
        std::slice::from_ref(&bold)
    ));
    match &content {
        MessageContent::Text(text) => {
            assert_eq!(text.text, "Hello world");
            assert_eq!(text.entities, vec![bold]);
        }
        other => panic!("unexpected content {other:?}"),
    }
    let mut empty = MessageContent::Text(TextContent {
        text: String::new(),
        entities: Vec::new(),
        link_preview: None,
    });
    assert!(super::translatable_content(&empty).is_none());
    // Content with no text is left alone.
    let mut sticker_like = MessageContent::ChatTtlChanged { secs: 0 };
    assert!(!super::replace_content_text(&mut sticker_like, "x", &[]));
    let _ = &mut empty;
}

#[test]
fn auto_translate_tracks_without_premium() {
    assert!(!tracking_enabled(true, false, false));
    assert!(tracking_enabled(true, true, false));
    assert!(tracking_enabled(true, false, true));
    assert!(!tracking_enabled(false, true, true));
    assert_eq!(
        bar_label_for(true, "en", true, Some("es")),
        "View Original (Spanish)"
    );
    assert_eq!(
        bar_label_for(true, "en", false, Some("es")),
        "Show Original"
    );
    assert_eq!(
        bar_label_for(false, "en", true, Some("es")),
        "Translate to English"
    );
}

#[test]
fn bar_label_follows_translation_state() {
    assert_eq!(bar_label(false, "en"), "Translate to English");
    assert_eq!(bar_label(true, "en"), "Show Original");
}
