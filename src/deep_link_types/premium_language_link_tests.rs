use super::{DeepLinkUi, InternalLink, LinkRoute, SettingsTarget, parse_internal_link, route};
use serde_json::{Value, json};

fn parsed(value: Value) -> InternalLink {
    parse_internal_link(&value).expect("internal link")
}

fn route_of(value: Value) -> LinkRoute {
    route(&parsed(value), "https://t.me/orig")
}

#[test]
fn gift_code_link_opens_the_box_with_its_code() {
    assert_eq!(
        route_of(json!({"@type":"internalLinkTypePremiumGiftCode","code":"AbC123"})),
        LinkRoute::Ui(DeepLinkUi::GiftCode {
            code: "AbC123".into()
        })
    );
    assert!(matches!(
        route_of(json!({"@type":"internalLinkTypePremiumGiftCode","code":""})),
        LinkRoute::Message(_)
    ));
}

#[test]
fn language_pack_link_shows_the_pack_and_changes_nothing() {
    assert_eq!(
        route_of(json!({"@type":"internalLinkTypeLanguagePack","language_pack_id":"pt-br"})),
        LinkRoute::Ui(DeepLinkUi::LanguagePack { id: "pt-br".into() })
    );
    assert!(matches!(
        route_of(json!({"@type":"internalLinkTypeLanguagePack","language_pack_id":""})),
        LinkRoute::Message(_)
    ));
}

#[test]
fn premium_offer_privacy_policy_and_language_settings_links() {
    assert_eq!(
        route_of(json!({"@type":"internalLinkTypePremiumFeaturesPage","referrer":"deeplink"})),
        LinkRoute::Ui(DeepLinkUi::Settings(SettingsTarget::Premium))
    );
    let policy = route_of(
        json!({"@type":"internalLinkTypeSettings","section":{"@type":"settingsSectionPrivacyPolicy"}}),
    );
    assert!(matches!(&policy, LinkRoute::Message(text) if text.contains("telegram.org/privacy")));
    let language = route_of(
        json!({"@type":"internalLinkTypeSettings","section":{"@type":"settingsSectionLanguage","subsection":""}}),
    );
    assert!(matches!(&language, LinkRoute::Message(text) if text.contains("English")));
}
