use super::*;
use crate::ai_compose::TextCompositionStyle;

fn result(text: &str) -> AiResult {
    AiResult {
        text: text.into(),
        entities: Vec::new(),
        diff: None,
    }
}

fn style(name: &str, creator_user_id: i64) -> TextCompositionStyle {
    TextCompositionStyle {
        name: name.into(),
        custom_emoji_id: 0,
        title: name.into(),
        is_custom: true,
        is_creator: false,
        install_count: 0,
        prompt: String::new(),
        creator_user_id,
        english_example: None,
    }
}

fn styles(names: &[(&str, i64)]) -> StyleList {
    let mut list = StyleList::default();
    list.set_server(names.iter().map(|(n, id)| style(n, *id)).collect());
    list
}

fn opened() -> (AiComposeBox, AiSend) {
    let mut ai = AiComposeBox::new("Hello there".into(), Some("en"), "de");
    assert_eq!(ai.start(), None);
    let send = ai
        .set_mode(AiComposeMode::Translate)
        .expect("the Translate tab asks at once");
    (ai, send)
}

fn token(send: &Option<AiSend>) -> u64 {
    send.as_ref().map(|send| send.token).expect("a request")
}

#[test]
fn opening_waits_on_the_style_tab() {
    let mut ai = AiComposeBox::new("Hello there".into(), Some("en"), "de");
    assert_eq!(ai.start(), None);
    assert_eq!(ai.mode(), AiComposeMode::Style);
    assert_eq!(ai.card(), &CardState::Waiting);
    assert_eq!(ai.footer(), FooterAction::SelectStyle);
    assert!(!ai.original_visible());
}

#[test]
fn translate_tab_targets_the_app_language() {
    let (ai, send) = opened();
    assert_eq!(ai.mode(), AiComposeMode::Translate);
    assert_eq!(ai.card(), &CardState::Loading { token: send.token });
    assert_eq!(
        send.request,
        AiRequest::Compose {
            text: "Hello there".into(),
            translate_to: "de".into(),
            style_name: String::new(),
            add_emojis: false,
        }
    );
    assert!(ai.original_visible());
    assert_eq!(ai.result_title(), ResultTitle::ToLanguage("de".into()));
}

#[test]
fn default_translate_target_follows_tdesktop() {
    let cases = [
        (Some("he"), "en", "en"),
        (Some("en"), "en", "es"),
        (Some("en"), "en-US", "es"),
        (Some("ru"), "pt-br", "pt"),
        (Some("pt"), "pt-BR", "en"),
        (None, "de", "de"),
        (None, "", "en"),
        (Some("en"), "", "es"),
        (Some("zh-Hans"), "zh-hant", "en"),
    ];
    for (detected, app, expected) in cases {
        assert_eq!(
            default_translate_to(detected, app),
            expected,
            "detected {detected:?}, app {app:?}"
        );
    }
}

#[test]
fn answers_only_count_for_the_current_token() {
    let (mut ai, first) = opened();
    let second = ai.set_translate_to("fr");
    assert_ne!(token(&second), first.token);
    assert_eq!(ai.accept(first.token, result("Hallo")), None);
    assert!(ai.loading(), "stale answer ignored");
    assert_eq!(ai.fail(first.token, AiBoxError::TooLong), None);
    assert!(ai.loading(), "stale error ignored");
    assert_eq!(ai.accept(token(&second), result("Bonjour")), None);
    assert_eq!(ai.result().map(|r| r.text.as_str()), Some("Bonjour"));
    assert_eq!(ai.result_title(), ResultTitle::ToLanguage("fr".into()));
}

#[test]
fn empty_answer_fails_with_the_generic_error() {
    let (mut ai, send) = opened();
    assert_eq!(
        ai.accept(send.token, result("")),
        Some(AiBoxError::Other(String::new()))
    );
    assert_eq!(ai.card(), &CardState::Failed);
    assert_eq!(ai.result_title(), ResultTitle::Original);
}

#[test]
fn flood_premium_turns_the_footer_into_increase_limit() {
    let (mut ai, send) = opened();
    assert_eq!(
        ai.fail(send.token, AiBoxError::FloodPremium),
        Some(AiBoxError::FloodPremium)
    );
    assert!(ai.premium_flooded());
    assert_eq!(ai.footer(), FooterAction::IncreaseLimit);
    let again = ai.set_mode(AiComposeMode::Fix);
    ai.accept(token(&again), result("fixed"));
    assert_eq!(
        ai.footer(),
        FooterAction::IncreaseLimit,
        "stays for the box"
    );
}

#[test]
fn other_errors_fail_the_card_and_come_back_for_the_toast() {
    let (mut ai, send) = opened();
    assert_eq!(
        ai.fail(send.token, AiBoxError::Other("SOME_ERROR".into())),
        Some(AiBoxError::Other("SOME_ERROR".into()))
    );
    assert_eq!(ai.card(), &CardState::Failed);
    assert!(!ai.premium_flooded());
}

#[test]
fn style_tab_waits_for_a_style() {
    let (mut ai, _) = opened();
    assert_eq!(ai.set_mode(AiComposeMode::Style), None);
    assert_eq!(ai.card(), &CardState::Waiting);
    assert_eq!(ai.footer(), FooterAction::SelectStyle);
    assert!(ai.emojify_visible(), "emojify alone can start a request");
    assert_eq!(ai.result_title(), ResultTitle::Original);
    let send = ai.select_style("pirate");
    assert_eq!(
        send.as_ref().map(|send| &send.request),
        Some(&AiRequest::Compose {
            text: "Hello there".into(),
            translate_to: String::new(),
            style_name: "pirate".into(),
            add_emojis: false,
        })
    );
    assert_eq!(
        ai.footer(),
        FooterAction::Apply {
            style: true,
            enabled: false
        }
    );
    ai.accept(token(&send), result("Ahoy there"));
    assert_eq!(
        ai.footer(),
        FooterAction::Apply {
            style: true,
            enabled: true
        }
    );
    assert_eq!(ai.result_title(), ResultTitle::Result);
}

#[test]
fn reselecting_the_active_style_asks_again() {
    let (mut ai, _) = opened();
    ai.set_mode(AiComposeMode::Style);
    let first = ai.select_style("pirate");
    let second = ai.select_style("pirate");
    assert_ne!(token(&first), token(&second));
}

#[test]
fn emojify_alone_composes_in_the_style_tab() {
    let (mut ai, _) = opened();
    ai.set_mode(AiComposeMode::Style);
    let send = ai.set_add_emojis(true);
    assert_eq!(
        send.map(|send| send.request),
        Some(AiRequest::Compose {
            text: "Hello there".into(),
            translate_to: String::new(),
            style_name: String::new(),
            add_emojis: true,
        })
    );
    assert_eq!(ai.footer(), FooterAction::SelectStyle, "still no style");
    assert_eq!(ai.set_add_emojis(false), None);
    assert_eq!(ai.card(), &CardState::Waiting);
}

#[test]
fn fix_ignores_emojify_and_hides_it() {
    let (mut ai, _) = opened();
    let send = ai.set_mode(AiComposeMode::Fix);
    assert_eq!(
        send.as_ref().map(|send| &send.request),
        Some(&AiRequest::Fix {
            text: "Hello there".into()
        })
    );
    assert_eq!(ai.set_add_emojis(true), None);
    assert!(ai.add_emojis());
    assert!(!ai.emojify_visible());
    ai.accept(token(&send), result("Hello, there"));
    assert!(!ai.emojify_visible());
    assert_eq!(
        ai.footer(),
        FooterAction::Apply {
            style: false,
            enabled: true
        }
    );
}

#[test]
fn emojify_requests_with_emoji_in_translate() {
    let (mut ai, _) = opened();
    let send = ai.set_add_emojis(true);
    assert_eq!(
        send.map(|send| send.request),
        Some(AiRequest::Compose {
            text: "Hello there".into(),
            translate_to: "de".into(),
            style_name: String::new(),
            add_emojis: true,
        })
    );
    assert!(ai.emojify_visible());
}

#[test]
fn leaving_the_style_tab_drops_the_style() {
    let (mut ai, _) = opened();
    ai.set_mode(AiComposeMode::Style);
    ai.select_style("pirate");
    ai.set_mode(AiComposeMode::Translate);
    assert_eq!(ai.style(), &StyleChoice::None);
    assert_eq!(ai.set_mode(AiComposeMode::Style), None);
    assert_eq!(ai.set_mode(AiComposeMode::Style), None, "same tab: nothing");
}

#[test]
fn translate_style_names_the_title_and_the_request() {
    let (mut ai, _) = opened();
    let send = ai.select_translate_style(StyleChoice::Style("formal".into()));
    assert_eq!(
        send.as_ref().map(|send| &send.request),
        Some(&AiRequest::Compose {
            text: "Hello there".into(),
            translate_to: "de".into(),
            style_name: "formal".into(),
            add_emojis: false,
        })
    );
    assert_eq!(
        ai.result_title(),
        ResultTitle::ToLanguageStyle("de".into(), "formal".into())
    );
}

#[test]
fn translate_choices_outside_translate_wait_for_the_tab() {
    let (mut ai, _) = opened();
    ai.set_mode(AiComposeMode::Fix);
    assert_eq!(ai.set_translate_to("it"), None);
    assert_eq!(ai.translate_to(), "it");
    let send = ai.set_mode(AiComposeMode::Translate);
    assert!(matches!(
        send.map(|send| send.request),
        Some(AiRequest::Compose { translate_to, .. }) if translate_to == "it"
    ));
}

#[test]
fn footer_table_matches_rebuild_buttons() {
    let (mut flooded, send) = opened();
    flooded.fail(send.token, AiBoxError::FloodPremium);
    let (loading_translate, _) = opened();
    let (mut ready_translate, send) = opened();
    ready_translate.accept(send.token, result("Hallo"));
    let (mut failed_translate, send) = opened();
    failed_translate.fail(send.token, AiBoxError::TooLong);
    let (mut waiting_style, _) = opened();
    waiting_style.set_mode(AiComposeMode::Style);
    let (mut loading_style, _) = opened();
    loading_style.set_mode(AiComposeMode::Style);
    loading_style.select_style("pirate");
    let cases = [
        (flooded, FooterAction::IncreaseLimit),
        (
            loading_translate,
            FooterAction::Apply {
                style: false,
                enabled: false,
            },
        ),
        (
            ready_translate,
            FooterAction::Apply {
                style: false,
                enabled: true,
            },
        ),
        (
            failed_translate,
            FooterAction::Apply {
                style: false,
                enabled: false,
            },
        ),
        (waiting_style, FooterAction::SelectStyle),
        (
            loading_style,
            FooterAction::Apply {
                style: true,
                enabled: false,
            },
        ),
    ];
    for (index, (ai, expected)) in cases.into_iter().enumerate() {
        assert_eq!(ai.footer(), expected, "case {index}");
    }
}

#[test]
fn author_shows_only_under_a_style_result_with_a_creator() {
    let list = styles(&[("pirate", 42), ("poet", 0)]);
    let (mut ai, send) = opened();
    ai.accept(send.token, result("Hallo"));
    assert_eq!(ai.author_user_id(&list), None, "translate result");
    ai.set_mode(AiComposeMode::Style);
    let send = ai.select_style("pirate");
    assert_eq!(ai.author_user_id(&list), None, "still loading");
    ai.accept(token(&send), result("Ahoy"));
    assert_eq!(ai.author_user_id(&list), Some(42));
    let send = ai.select_style("poet");
    ai.accept(token(&send), result("Hark"));
    assert_eq!(ai.author_user_id(&list), None, "no creator");
}

#[test]
fn a_style_the_server_dropped_is_unpicked() {
    let (mut ai, _) = opened();
    ai.select_translate_style(StyleChoice::Style("formal".into()));
    ai.set_mode(AiComposeMode::Style);
    ai.select_style("pirate");
    let kept = styles(&[("pirate", 0), ("formal", 0)]);
    assert_eq!(ai.refresh_styles(&kept), None);
    assert_eq!(ai.style(), &StyleChoice::Style("pirate".into()));
    let dropped = styles(&[("poet", 0)]);
    assert_eq!(ai.refresh_styles(&dropped), None, "back to waiting");
    assert_eq!(ai.style(), &StyleChoice::None);
    assert_eq!(ai.translate_style(), &StyleChoice::None);
    assert_eq!(ai.card(), &CardState::Waiting);
}

#[test]
fn original_toggle_flips() {
    let (mut ai, _) = opened();
    assert!(!ai.original_expanded());
    ai.toggle_original_expanded();
    assert!(ai.original_expanded());
}

#[test]
fn ai_button_needs_styles_three_lines_and_real_text() {
    let cases = [
        ("one\ntwo\nthree", 3, true, 4096, true),
        ("one\ntwo\nthree", 2, true, 4096, false),
        ("one\ntwo\nthree", 3, false, 4096, false),
        ("   \n\t\n ", 3, true, 4096, false),
        ("😀😀", 3, true, 3, false),
        ("😀😀", 3, true, 4, true),
    ];
    for (text, lines, has_styles, max, expected) in cases {
        assert_eq!(
            ai_button_visible(text, lines, has_styles, max),
            expected,
            "{text:?} lines {lines} styles {has_styles} max {max}"
        );
    }
}
