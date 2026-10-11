use super::{
    DeepLinkAction, DeepLinkUi, InternalLink, LinkRoute, SettingsTarget, invite_hash,
    login_code_from_link, needs_internal_resolution, parse_internal_link, route,
};
use serde_json::{Value, json};

fn parsed(value: Value) -> InternalLink {
    parse_internal_link(&value).expect("internal link")
}

#[test]
fn parses_every_handled_type() {
    assert_eq!(
        parsed(
            json!({"@type":"internalLinkTypeStickerSet","sticker_set_name":"Cats","expect_custom_emoji":false})
        ),
        InternalLink::StickerSet {
            name: "Cats".into(),
            custom_emoji: false
        }
    );
    assert_eq!(
        parsed(json!({"@type":"internalLinkTypeProxy","proxy":{"server":"x"}})),
        InternalLink::Proxy
    );
    assert_eq!(
        parsed(
            json!({"@type":"internalLinkTypeMessageDraft","text":{"@type":"formattedText","text":"hi https://x.y","entities":[]},"contains_link":true})
        ),
        InternalLink::MessageDraft {
            text: "hi https://x.y".into()
        }
    );
    assert_eq!(
        parsed(
            json!({"@type":"internalLinkTypeSettings","section":{"@type":"settingsSectionPrivacyAndSecurity","subsection":""}})
        ),
        InternalLink::Settings(SettingsTarget::PrivacyAndSecurity)
    );
    assert_eq!(
        parsed(json!({"@type":"internalLinkTypeAuthenticationCode","code":"12345"})),
        InternalLink::AuthenticationCode {
            code: "12345".into()
        }
    );
    assert_eq!(
        parsed(
            json!({"@type":"internalLinkTypeUserPhoneNumber","phone_number":"15550001","draft_text":"yo","open_profile":false})
        ),
        InternalLink::UserPhoneNumber {
            phone: "15550001".into(),
            draft_text: "yo".into()
        }
    );
    assert_eq!(
        parsed(
            json!({"@type":"internalLinkTypeStory","story_poster_username":"durov","story_id":7})
        ),
        InternalLink::Story {
            username: "durov".into(),
            story_id: 7
        }
    );
    assert_eq!(
        parsed(
            json!({"@type":"internalLinkTypeVideoChat","chat_username":"g","invite_hash":"","is_live_stream":true})
        ),
        InternalLink::VideoChat {
            username: "g".into(),
            live_stream: true
        }
    );
    assert_eq!(
        parsed(json!({"@type":"internalLinkTypeQrCodeAuthentication"})),
        InternalLink::Other("QrCodeAuthentication".into())
    );
    assert_eq!(parse_internal_link(&json!({"@type":"ok"})), None);
}

#[test]
fn invite_hashes_come_from_every_spelling() {
    assert_eq!(invite_hash("https://t.me/+AbC"), Some("AbC".into()));
    assert_eq!(invite_hash("https://t.me/joinchat/AbC"), Some("AbC".into()));
    assert_eq!(invite_hash("tg://join?invite=AbC&x=1"), Some("AbC".into()));
    assert_eq!(invite_hash("https://t.me/+"), None);
}

use crate::bot_invite::{Invite, Scope};

fn route_of(value: Value) -> LinkRoute {
    route(&parsed(value), "https://t.me/orig")
}

#[test]
fn chat_links_route_to_the_existing_chat_flow() {
    assert_eq!(
        route_of(
            json!({"@type":"internalLinkTypePublicChat","chat_username":"durov","draft_text":"","open_profile":false})
        ),
        LinkRoute::Resolve(DeepLinkAction::OpenUsername {
            domain: "durov".into(),
            start_param: None,
            post: None,
            story_id: None
        })
    );
    assert_eq!(
        route_of(
            json!({"@type":"internalLinkTypePublicChat","chat_username":"durov","draft_text":"hello","open_profile":false})
        ),
        LinkRoute::Resolve(DeepLinkAction::OpenPublicChatDraft {
            domain: "durov".into(),
            draft: "hello".into()
        })
    );
    assert_eq!(
        route_of(
            json!({"@type":"internalLinkTypeBotStart","bot_username":"bot","start_parameter":"p","autostart":true})
        ),
        LinkRoute::Resolve(DeepLinkAction::OpenUsername {
            domain: "bot".into(),
            start_param: Some("p".into()),
            post: None,
            story_id: None
        })
    );
    assert_eq!(
        route_of(json!({"@type":"internalLinkTypeChatInvite","invite_link":"https://t.me/+Zz"})),
        LinkRoute::Resolve(DeepLinkAction::JoinInvite { hash: "Zz".into() })
    );
    assert_eq!(
        route_of(json!({"@type":"internalLinkTypeMessage","url":"https://t.me/c/1/2?thread=3"})),
        LinkRoute::Resolve(DeepLinkAction::MessageLink {
            url: "https://t.me/c/1/2?thread=3".into()
        })
    );
    assert_eq!(
        route_of(
            json!({"@type":"internalLinkTypeStory","story_poster_username":"durov","story_id":7})
        ),
        LinkRoute::Resolve(DeepLinkAction::OpenUsername {
            domain: "durov".into(),
            start_param: None,
            post: None,
            story_id: Some(7)
        })
    );
    assert_eq!(
        route_of(
            json!({"@type":"internalLinkTypeStickerSet","sticker_set_name":"Cats","expect_custom_emoji":true})
        ),
        LinkRoute::Resolve(DeepLinkAction::StickerSet {
            name: "Cats".into()
        })
    );
    assert_eq!(
        route_of(
            json!({"@type":"internalLinkTypeUserPhoneNumber","phone_number":"+1 (555) 0001","draft_text":"","open_profile":false})
        ),
        LinkRoute::Resolve(DeepLinkAction::UserPhone {
            phone: "15550001".into(),
            draft: String::new()
        })
    );
}

#[test]
fn ui_links_route_without_a_request() {
    assert_eq!(
        route_of(json!({"@type":"internalLinkTypeProxy","proxy":{}})),
        LinkRoute::Ui(DeepLinkUi::Proxy {
            link: "https://t.me/orig".into()
        })
    );
    assert_eq!(
        route_of(
            json!({"@type":"internalLinkTypeMessageDraft","text":{"text":"look https://a.b"},"contains_link":true})
        ),
        LinkRoute::Ui(DeepLinkUi::Share {
            text: "look https://a.b".into()
        })
    );
    assert_eq!(
        route_of(
            json!({"@type":"internalLinkTypeSettings","section":{"@type":"settingsSectionAppearance"}})
        ),
        LinkRoute::Ui(DeepLinkUi::Settings(SettingsTarget::Appearance))
    );
    assert_eq!(
        route_of(
            json!({"@type":"internalLinkTypeSettings","section":{"@type":"settingsSectionPremium"}})
        ),
        LinkRoute::Ui(DeepLinkUi::Settings(SettingsTarget::Premium))
    );
    assert_eq!(
        route_of(json!({"@type":"internalLinkTypeContactsPage","section":""})),
        LinkRoute::Ui(DeepLinkUi::Settings(SettingsTarget::Contacts))
    );
}

#[test]
fn share_drafts_that_start_with_an_at_sign_are_refused() {
    // tdesktop `ShareUrl`: no inline bot queries through a share link.
    assert!(matches!(
        route_of(
            json!({"@type":"internalLinkTypeMessageDraft","text":{"text":" @gif cats"},"contains_link":false})
        ),
        LinkRoute::Message(_)
    ));
    assert!(matches!(
        route_of(
            json!({"@type":"internalLinkTypeMessageDraft","text":{"text":"  "},"contains_link":false})
        ),
        LinkRoute::Message(_)
    ));
}

#[test]
fn background_links_open_the_wallpaper_preview() {
    assert_eq!(
        route_of(json!({"@type":"internalLinkTypeBackground","background_name":"sky"})),
        LinkRoute::Ui(DeepLinkUi::Background { name: "sky".into() })
    );
    assert!(matches!(
        route_of(json!({"@type":"internalLinkTypeBackground","background_name":""})),
        LinkRoute::Message(_)
    ));
}

#[test]
fn folder_invite_links_open_the_add_folder_box() {
    assert_eq!(
        route_of(json!({
            "@type":"internalLinkTypeChatFolderInvite",
            "invite_link":"https://t.me/addlist/abc"
        })),
        LinkRoute::Ui(DeepLinkUi::FolderInvite {
            link: "https://t.me/addlist/abc".into()
        })
    );
    assert!(matches!(
        route_of(json!({"@type":"internalLinkTypeChatFolderInvite","invite_link":""})),
        LinkRoute::Message(_)
    ));
}

#[test]
fn boost_and_premium_links_route_to_their_screens() {
    assert_eq!(
        route(
            &parsed(json!({"@type":"internalLinkTypeChatBoost","url":"https://t.me/c/1/?boost"})),
            ""
        ),
        LinkRoute::Resolve(DeepLinkAction::BoostLink {
            url: "https://t.me/c/1/?boost".into()
        })
    );
    assert!(matches!(
        route(
            &parsed(json!({"@type":"internalLinkTypeChatBoost","url":""})),
            ""
        ),
        LinkRoute::Message(_)
    ));
    assert_eq!(
        route(
            &parsed(json!({"@type":"internalLinkTypePremiumFeaturesPage","referrer":"x"})),
            ""
        ),
        LinkRoute::Ui(DeepLinkUi::Settings(SettingsTarget::Premium))
    );
}

#[test]
fn unsupported_targets_say_so_and_never_open_a_browser() {
    for ty in [
        "internalLinkTypeInvoice",
        "internalLinkTypePremiumGiftCode",
        "internalLinkTypeVideoChat",
        "internalLinkTypeGroupCall",
        "internalLinkTypeTheme",
        "internalLinkTypeAuthenticationCode",
        "internalLinkTypeQrCodeAuthentication",
    ] {
        assert!(
            matches!(route_of(json!({"@type": ty})), LinkRoute::Message(_)),
            "{ty}"
        );
    }
}

fn admin_rights_json() -> Value {
    json!({"@type":"chatAdministratorRights","can_manage_chat":true,"can_delete_messages":true})
}

#[test]
fn startgroup_links_pick_the_scope_from_the_requested_rights() {
    let plain = route_of(json!({
        "@type":"internalLinkTypeBotStartInGroup","bot_username":"helper",
        "start_parameter":"x","administrator_rights":null
    }));
    assert_eq!(
        plain,
        LinkRoute::Resolve(DeepLinkAction::AddBot {
            domain: "helper".into(),
            invite: Invite {
                scope: Scope::All,
                requested_rights: None,
                start_parameter: "x".into(),
            },
        })
    );
    let admin = route_of(json!({
        "@type":"internalLinkTypeBotStartInGroup","bot_username":"helper",
        "start_parameter":"","administrator_rights":admin_rights_json()
    }));
    let LinkRoute::Resolve(DeepLinkAction::AddBot { invite, .. }) = admin else {
        panic!("expected an add-bot route");
    };
    assert_eq!(invite.scope, Scope::GroupAdmin);
    let rights = invite.requested_rights.expect("rights");
    assert!(rights.can_manage_chat && rights.can_delete_messages && !rights.can_pin_messages);
}

#[test]
fn startchannel_links_ask_for_channels() {
    let route = route_of(json!({
        "@type":"internalLinkTypeBotAddToChannel","bot_username":"helper",
        "administrator_rights":admin_rights_json()
    }));
    let LinkRoute::Resolve(DeepLinkAction::AddBot { domain, invite }) = route else {
        panic!("expected an add-bot route");
    };
    assert_eq!(domain, "helper");
    assert_eq!(invite.scope, Scope::ChannelAdmin);
    assert!(invite.requested_rights.is_some());
}

#[test]
fn game_links_open_the_share_picker() {
    assert_eq!(
        route_of(json!({
            "@type":"internalLinkTypeGame","bot_username":"chessbot","game_short_name":"chess"
        })),
        LinkRoute::Resolve(DeepLinkAction::ShareGame {
            domain: "chessbot".into(),
            game_short_name: "chess".into(),
        })
    );
    assert!(matches!(
        route_of(
            json!({"@type":"internalLinkTypeGame","bot_username":"chessbot","game_short_name":""})
        ),
        LinkRoute::Message(_)
    ));
}

#[test]
fn unknown_deep_links_fall_back_to_deep_link_info() {
    assert_eq!(
        route_of(json!({"@type":"internalLinkTypeUnknownDeepLink","link":"tg://nope"})),
        LinkRoute::Unknown("tg://nope".into())
    );
    assert_eq!(
        route_of(json!({"@type":"internalLinkTypeUnknownDeepLink","link":""})),
        LinkRoute::Unknown("https://t.me/orig".into())
    );
}

#[test]
fn extended_params_skip_the_local_fast_path() {
    for link in [
        "https://t.me/ch/5?comment=7",
        "https://t.me/c/1/5?thread=3",
        "https://t.me/ch/5?t=30",
        "https://t.me/bot?startgroup=x",
        "tg://resolve?domain=bot&startgroup",
        "tg://resolve?domain=ch&post=5&comment=7",
        "https://t.me/durov?text=hi",
    ] {
        assert!(needs_internal_resolution(link), "{link}");
    }
    for link in [
        "https://t.me/durov",
        "https://t.me/durov/42?single",
        "https://t.me/bot?start=abc",
        "tg://resolve?domain=durov&start=x&post=1",
        "https://t.me/durov#t=3",
    ] {
        assert!(!needs_internal_resolution(link), "{link}");
    }
}

#[test]
fn login_codes_come_from_the_two_link_forms_only() {
    assert_eq!(
        login_code_from_link("tg://login?code=12345"),
        Some("12345".into())
    );
    assert_eq!(
        login_code_from_link("tg://login/?code=12345&x=1"),
        Some("12345".into())
    );
    assert_eq!(
        login_code_from_link("TG://LOGIN?CODE=99999"),
        Some("99999".into())
    );
    assert_eq!(
        login_code_from_link("https://t.me/login/54321"),
        Some("54321".into())
    );
    assert_eq!(
        login_code_from_link("http://telegram.me/login/54321?x"),
        Some("54321".into())
    );
    for bad in [
        "tg://login",
        "tg://login?code=",
        "tg://login?code=12a45",
        "tg://login?code=123456789",
        "tg://login?code=-1",
        "tg://resolve?domain=login&code=1",
        "https://evil.example/login/12345",
        "https://t.me/login/",
        "https://t.me/durov",
    ] {
        assert_eq!(login_code_from_link(bad), None, "{bad}");
    }
}
