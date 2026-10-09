//! `getInternalLinkType` answers and where each kind of link goes
//! (`parity:deeplink-internal-link-type`).
//!
//! Telegram Desktop matches the link text against a list of regexes in
//! `core/local_url_handlers.cpp` and hands anything it does not know to
//! `getDeepLinkInfo` (`HandleUnknown`). Quill asks TDLib instead: the answer
//! is one `internalLinkType*` object, parsed here into [`InternalLink`], and
//! [`route`] decides what the app does with it. Everything in this module is
//! pure so every platform tests it without a network.

use crate::state::DeepLinkAction;
use serde_json::Value;

/// One `InternalLinkType` answer, reduced to the fields Quill acts on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InternalLink {
    PublicChat {
        username: String,
        draft_text: String,
        open_profile: bool,
    },
    BotStart {
        username: String,
        start_parameter: String,
    },
    BotStartInGroup {
        username: String,
    },
    BotAddToChannel {
        username: String,
    },
    ChatInvite {
        invite_link: String,
    },
    Message {
        url: String,
    },
    Story {
        username: String,
        story_id: i32,
    },
    StickerSet {
        name: String,
        custom_emoji: bool,
    },
    Proxy,
    MessageDraft {
        text: String,
    },
    Settings(SettingsTarget),
    AuthenticationCode {
        code: String,
    },
    Invoice {
        name: String,
    },
    ChatBoost,
    PremiumGiftCode,
    VideoChat {
        live_stream: bool,
    },
    GroupCall,
    ChatFolderInvite,
    Background,
    Theme,
    UserPhoneNumber {
        phone: String,
        draft_text: String,
    },
    SavedMessages,
    UnknownDeepLink {
        link: String,
    },
    /// Any other `internalLinkType*` (kept by name for the message).
    Other(String),
}

/// Where a `settings` link lands. Mirrors tdesktop's settings router
/// (`core/deep_links/deep_links_settings.cpp`): each section opens its page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTarget {
    Root,
    Appearance,
    ChatFolders,
    DataAndStorage,
    Devices,
    EditProfile,
    Notifications,
    PrivacyAndSecurity,
    Contacts,
    Calls,
    NewGroup,
    NewChannel,
    SavedMessages,
    /// A page Quill has no screen for; the settings list opens instead.
    Unsupported,
}

/// What the UI does once the link is understood.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeepLinkUi {
    /// `addstickers` / `addemoji`: the set preview with Add (set id).
    StickerSet {
        set_id: i64,
    },
    /// `msg` / `msg_url` / message draft: pick a chat, prefill the composer.
    Share {
        text: String,
    },
    /// `proxy` / `socks`: the existing proxy confirmation (original link).
    Proxy {
        link: String,
    },
    Settings(SettingsTarget),
}

/// The decision for one resolved link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkRoute {
    /// Needs a follow-up request that ends in a chat.
    Resolve(DeepLinkAction),
    /// Needs no request; the UI acts on it.
    Ui(DeepLinkUi),
    /// TDLib calls it unknown: ask `getDeepLinkInfo`, as tdesktop does.
    Unknown(String),
    /// Quill has no screen yet: this text goes in a dialog.
    Message(String),
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn flag(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn settings_target(section: Option<&str>) -> SettingsTarget {
    match section.unwrap_or("settingsSectionFeatures") {
        "settingsSectionAppearance" => SettingsTarget::Appearance,
        "settingsSectionChatFolders" => SettingsTarget::ChatFolders,
        "settingsSectionDataAndStorage" => SettingsTarget::DataAndStorage,
        "settingsSectionDevices" => SettingsTarget::Devices,
        "settingsSectionEditProfile" => SettingsTarget::EditProfile,
        "settingsSectionNotifications" => SettingsTarget::Notifications,
        "settingsSectionPrivacyAndSecurity" => SettingsTarget::PrivacyAndSecurity,
        _ => SettingsTarget::Unsupported,
    }
}

/// An `internalLinkType*` object → [`InternalLink`]. `None` when the value
/// is not an internal link type at all.
pub fn parse_internal_link(value: &Value) -> Option<InternalLink> {
    let kind = value
        .get("@type")?
        .as_str()?
        .strip_prefix("internalLinkType")?;
    Some(match kind {
        "PublicChat" => InternalLink::PublicChat {
            username: text(value, "chat_username"),
            draft_text: text(value, "draft_text"),
            open_profile: flag(value, "open_profile"),
        },
        "BotStart" => InternalLink::BotStart {
            username: text(value, "bot_username"),
            start_parameter: text(value, "start_parameter"),
        },
        "BotStartInGroup" => InternalLink::BotStartInGroup {
            username: text(value, "bot_username"),
        },
        "BotAddToChannel" => InternalLink::BotAddToChannel {
            username: text(value, "bot_username"),
        },
        "ChatInvite" => InternalLink::ChatInvite {
            invite_link: text(value, "invite_link"),
        },
        "Message" => InternalLink::Message {
            url: text(value, "url"),
        },
        "Story" => InternalLink::Story {
            username: text(value, "story_poster_username"),
            story_id: value
                .get("story_id")
                .and_then(Value::as_i64)
                .and_then(|id| i32::try_from(id).ok())
                .unwrap_or(0),
        },
        "StickerSet" => InternalLink::StickerSet {
            name: text(value, "sticker_set_name"),
            custom_emoji: flag(value, "expect_custom_emoji"),
        },
        "Proxy" => InternalLink::Proxy,
        "MessageDraft" => InternalLink::MessageDraft {
            text: value
                .get("text")
                .and_then(|t| t.get("text"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        },
        "Settings" => InternalLink::Settings(settings_target(
            value
                .get("section")
                .and_then(|s| s.get("@type"))
                .and_then(Value::as_str),
        )),
        "ContactsPage" => InternalLink::Settings(SettingsTarget::Contacts),
        "CallsPage" => InternalLink::Settings(SettingsTarget::Calls),
        "MyProfilePage" => InternalLink::Settings(SettingsTarget::EditProfile),
        "NewGroupChat" => InternalLink::Settings(SettingsTarget::NewGroup),
        "NewChannelChat" => InternalLink::Settings(SettingsTarget::NewChannel),
        "AuthenticationCode" => InternalLink::AuthenticationCode {
            code: text(value, "code"),
        },
        "Invoice" => InternalLink::Invoice {
            name: text(value, "invoice_name"),
        },
        "ChatBoost" => InternalLink::ChatBoost,
        "PremiumGiftCode" => InternalLink::PremiumGiftCode,
        "VideoChat" => InternalLink::VideoChat {
            live_stream: flag(value, "is_live_stream"),
        },
        "GroupCall" => InternalLink::GroupCall,
        "ChatFolderInvite" => InternalLink::ChatFolderInvite,
        "Background" => InternalLink::Background,
        "Theme" => InternalLink::Theme,
        "UserPhoneNumber" => InternalLink::UserPhoneNumber {
            phone: text(value, "phone_number"),
            draft_text: text(value, "draft_text"),
        },
        "SavedMessages" => InternalLink::SavedMessages,
        "UnknownDeepLink" => InternalLink::UnknownDeepLink {
            link: text(value, "link"),
        },
        other => InternalLink::Other(other.to_string()),
    })
}

/// The invite hash inside `t.me/+<hash>` / `t.me/joinchat/<hash>` /
/// `tg://join?invite=<hash>`.
pub fn invite_hash(invite_link: &str) -> Option<String> {
    let hash = if let Some((_, rest)) = invite_link.split_once("invite=") {
        rest.split('&').next().unwrap_or("")
    } else {
        let path = invite_link.split(['?', '#']).next().unwrap_or("");
        path.rsplit('/')
            .next()
            .unwrap_or("")
            .trim_start_matches('+')
    };
    (!hash.is_empty()).then(|| hash.to_string())
}

const NOT_SUPPORTED: &str = "isn't supported by Quill yet.";

/// Decide what to do with one resolved link. `original` is the text the
/// user opened; only the proxy hand-off needs it back.
pub fn route(link: &InternalLink, original: &str) -> LinkRoute {
    use DeepLinkAction as A;
    match link {
        InternalLink::PublicChat {
            username,
            draft_text,
            ..
        } => {
            if draft_text.is_empty() {
                LinkRoute::Resolve(A::OpenUsername {
                    domain: username.clone(),
                    start_param: None,
                    post: None,
                    story_id: None,
                })
            } else {
                LinkRoute::Resolve(A::OpenPublicChatDraft {
                    domain: username.clone(),
                    draft: draft_text.clone(),
                })
            }
        }
        InternalLink::BotStart {
            username,
            start_parameter,
        } => LinkRoute::Resolve(A::OpenUsername {
            domain: username.clone(),
            start_param: Some(start_parameter.clone()).filter(|s| !s.is_empty()),
            post: None,
            story_id: None,
        }),
        InternalLink::BotStartInGroup { .. } | InternalLink::BotAddToChannel { .. } => {
            LinkRoute::Message(
                "Adding a bot to a group or channel from a link isn't supported by Quill yet. \
                 Add the bot from the group's member list."
                    .into(),
            )
        }
        InternalLink::ChatInvite { invite_link } => match invite_hash(invite_link) {
            Some(hash) => LinkRoute::Resolve(A::JoinInvite { hash }),
            None => LinkRoute::Message("This invite link is broken or has expired.".into()),
        },
        InternalLink::Message { url } => LinkRoute::Resolve(A::MessageLink { url: url.clone() }),
        InternalLink::Story { username, story_id } => LinkRoute::Resolve(A::OpenUsername {
            domain: username.clone(),
            start_param: None,
            post: None,
            story_id: Some(*story_id).filter(|id| *id > 0),
        }),
        InternalLink::StickerSet { name, .. } => {
            LinkRoute::Resolve(A::StickerSet { name: name.clone() })
        }
        InternalLink::Proxy => LinkRoute::Ui(DeepLinkUi::Proxy {
            link: original.to_string(),
        }),
        InternalLink::MessageDraft { text } => {
            // tdesktop `ShareUrl`: a draft that starts with "@" would run an
            // inline bot query in the target chat, so it is refused.
            if text.trim().is_empty() || text.trim_start().starts_with('@') {
                LinkRoute::Message("This share link can't be used.".into())
            } else {
                LinkRoute::Ui(DeepLinkUi::Share { text: text.clone() })
            }
        }
        InternalLink::Settings(target) => LinkRoute::Ui(DeepLinkUi::Settings(*target)),
        InternalLink::AuthenticationCode { .. } => LinkRoute::Message(
            "This is a sign-in code link. Codes are only used while signing in, and you are \
             already signed in."
                .into(),
        ),
        InternalLink::Invoice { .. } => {
            LinkRoute::Message(format!("Paying an invoice from a link {NOT_SUPPORTED}"))
        }
        InternalLink::ChatBoost => {
            LinkRoute::Message(format!("Boosting a channel from a link {NOT_SUPPORTED}"))
        }
        InternalLink::PremiumGiftCode => LinkRoute::Message(format!("Gift codes {NOT_SUPPORTED}")),
        InternalLink::VideoChat { live_stream } => LinkRoute::Message(if *live_stream {
            format!("Joining a live stream from a link {NOT_SUPPORTED}")
        } else {
            format!("Joining a video chat from a link {NOT_SUPPORTED}")
        }),
        InternalLink::GroupCall => {
            LinkRoute::Message(format!("Joining a call from a link {NOT_SUPPORTED}"))
        }
        InternalLink::ChatFolderInvite => {
            LinkRoute::Message(format!("Folder invite links {NOT_SUPPORTED}"))
        }
        InternalLink::Background => {
            LinkRoute::Message(format!("Chat background links {NOT_SUPPORTED}"))
        }
        InternalLink::Theme => LinkRoute::Message(format!("Theme links {NOT_SUPPORTED}")),
        InternalLink::UserPhoneNumber { phone, draft_text } => {
            let phone: String = phone.chars().filter(char::is_ascii_digit).collect();
            if phone.is_empty() {
                LinkRoute::Message("This phone number link is broken.".into())
            } else {
                LinkRoute::Resolve(A::UserPhone {
                    phone,
                    draft: draft_text.clone(),
                })
            }
        }
        InternalLink::SavedMessages => {
            LinkRoute::Ui(DeepLinkUi::Settings(SettingsTarget::SavedMessages))
        }
        InternalLink::UnknownDeepLink { link } => LinkRoute::Unknown(if link.is_empty() {
            original.to_string()
        } else {
            link.clone()
        }),
        InternalLink::Other(_) => LinkRoute::Message(format!("This link {NOT_SUPPORTED}")),
    }
}

/// Query names the local parsers (`connect::deep_links`) do not understand.
/// A link carrying one goes through TDLib instead of the local fast path.
const TDLIB_ONLY_PARAMS: [&str; 17] = [
    "comment",
    "thread",
    "topic",
    "t",
    "startgroup",
    "startchannel",
    "boost",
    "livestream",
    "videochat",
    "voicechat",
    "startapp",
    "attach",
    "game",
    "text",
    "profile",
    "direct",
    "album",
];

/// Whether `link` has a query parameter that needs TDLib's parser.
pub fn needs_internal_resolution(link: &str) -> bool {
    let query = link
        .split('#')
        .next()
        .and_then(|l| l.split_once('?'))
        .map(|(_, q)| q)
        .unwrap_or("");
    query.split('&').any(|pair| {
        let name = pair.split('=').next().unwrap_or("").to_ascii_lowercase();
        TDLIB_ONLY_PARAMS.contains(&name.as_str())
    })
}

#[cfg(test)]
mod tests {
    use super::{
        DeepLinkAction, DeepLinkUi, InternalLink, LinkRoute, SettingsTarget, invite_hash,
        needs_internal_resolution, parse_internal_link, route,
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
            InternalLink::VideoChat { live_stream: true }
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
            route_of(
                json!({"@type":"internalLinkTypeChatInvite","invite_link":"https://t.me/+Zz"})
            ),
            LinkRoute::Resolve(DeepLinkAction::JoinInvite { hash: "Zz".into() })
        );
        assert_eq!(
            route_of(
                json!({"@type":"internalLinkTypeMessage","url":"https://t.me/c/1/2?thread=3"})
            ),
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
            LinkRoute::Ui(DeepLinkUi::Settings(SettingsTarget::Unsupported))
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
    fn unsupported_targets_say_so_and_never_open_a_browser() {
        for ty in [
            "internalLinkTypeInvoice",
            "internalLinkTypeChatBoost",
            "internalLinkTypePremiumGiftCode",
            "internalLinkTypeVideoChat",
            "internalLinkTypeGroupCall",
            "internalLinkTypeChatFolderInvite",
            "internalLinkTypeBackground",
            "internalLinkTypeTheme",
            "internalLinkTypeBotStartInGroup",
            "internalLinkTypeBotAddToChannel",
            "internalLinkTypeAuthenticationCode",
            "internalLinkTypeQrCodeAuthentication",
        ] {
            assert!(
                matches!(route_of(json!({"@type": ty})), LinkRoute::Message(_)),
                "{ty}"
            );
        }
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
}
