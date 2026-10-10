//! `getInternalLinkType` answers and where each kind of link goes
//! (`parity:deeplink-internal-link-type`).
//!
//! Telegram Desktop matches the link text against a list of regexes in
//! `core/local_url_handlers.cpp` and hands anything it does not know to
//! `getDeepLinkInfo` (`HandleUnknown`). Quill asks TDLib instead: the answer
//! is one `internalLinkType*` object, parsed here into [`InternalLink`], and
//! [`route`] decides what the app does with it. Everything in this module is
//! pure so every platform tests it without a network.

use crate::bot_invite::{Invite, Scope};
use crate::state::DeepLinkAction;
use crate::telegram::envelope::ChatAdminRights;
use crate::telegram::parse_chat_admin_rights;
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
        start_parameter: String,
        administrator_rights: Option<ChatAdminRights>,
    },
    BotAddToChannel {
        username: String,
        administrator_rights: Option<ChatAdminRights>,
    },
    /// `internalLinkTypeWebApp`: `t.me/<bot>/<app>?startapp=`.
    WebApp {
        username: String,
        short_name: String,
        start_parameter: String,
    },
    /// `internalLinkTypeMainWebApp`: `t.me/<bot>?startapp=`.
    MainWebApp {
        username: String,
        start_parameter: String,
    },
    /// `internalLinkTypeAttachmentMenuBot`: `t.me/<bot>?startattach=`.
    AttachmentMenuBot {
        username: String,
        url: String,
    },
    Game {
        username: String,
        game_short_name: String,
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
    ChatBoost {
        url: String,
    },
    PremiumGiftCode,
    VideoChat {
        username: String,
        live_stream: bool,
    },
    GroupCall,
    ChatFolderInvite {
        invite_link: String,
    },
    Background {
        name: String,
    },
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
    /// The Telegram Premium page (`internalLinkTypePremiumFeaturesPage`).
    Premium,
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
    /// `addlist`: the "Add folder" box for a shared folder (invite link).
    FolderInvite {
        link: String,
    },
    /// `bg`: the wallpaper preview for a background name (`searchBackground`).
    Background {
        name: String,
    },
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
            start_parameter: text(value, "start_parameter"),
            administrator_rights: parse_chat_admin_rights(value.get("administrator_rights"))
                .filter(|rights| *rights != ChatAdminRights::default()),
        },
        "BotAddToChannel" => InternalLink::BotAddToChannel {
            username: text(value, "bot_username"),
            administrator_rights: parse_chat_admin_rights(value.get("administrator_rights"))
                .filter(|rights| *rights != ChatAdminRights::default()),
        },
        "WebApp" => InternalLink::WebApp {
            username: text(value, "bot_username"),
            short_name: text(value, "web_app_short_name"),
            start_parameter: text(value, "start_parameter"),
        },
        "MainWebApp" => InternalLink::MainWebApp {
            username: text(value, "bot_username"),
            start_parameter: text(value, "start_parameter"),
        },
        "AttachmentMenuBot" => InternalLink::AttachmentMenuBot {
            username: text(value, "bot_username"),
            url: text(value, "url"),
        },
        "Game" => InternalLink::Game {
            username: text(value, "bot_username"),
            game_short_name: text(value, "game_short_name"),
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
        "ChatBoost" => InternalLink::ChatBoost {
            url: text(value, "url"),
        },
        "PremiumFeaturesPage" => InternalLink::Settings(SettingsTarget::Premium),
        "PremiumGiftCode" => InternalLink::PremiumGiftCode,
        "VideoChat" => InternalLink::VideoChat {
            username: text(value, "chat_username"),
            live_stream: flag(value, "is_live_stream"),
        },
        "GroupCall" => InternalLink::GroupCall,
        "ChatFolderInvite" => InternalLink::ChatFolderInvite {
            invite_link: text(value, "invite_link"),
        },
        "Background" => InternalLink::Background {
            name: text(value, "background_name"),
        },
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

/// The sign-in code in a login link: `tg://login?code=12345` (tdesktop
/// `ResolveLoginCode`) or `https://t.me/login/12345`. Digits only, so
/// nothing but a code can reach the sign-in field. Checked locally because
/// TDLib's `getInternalLinkType` needs a signed-in session to be asked.
pub fn login_code_from_link(link: &str) -> Option<String> {
    let lower = link.trim().to_ascii_lowercase();
    let raw = if let Some(rest) = lower.strip_prefix("tg://login") {
        let query = rest.trim_start_matches('/').strip_prefix('?')?;
        query
            .split('#')
            .next()?
            .split('&')
            .find_map(|pair| pair.strip_prefix("code="))?
            .to_string()
    } else {
        let rest = lower
            .strip_prefix("https://")
            .or_else(|| lower.strip_prefix("http://"))?;
        let (host, path) = rest.split_once('/')?;
        if !matches!(host, "t.me" | "telegram.me" | "telegram.dog") {
            return None;
        }
        path.strip_prefix("login/")?
            .split(['?', '#', '/'])
            .next()?
            .to_string()
    };
    (!raw.is_empty() && raw.len() <= MAX_LOGIN_CODE && raw.bytes().all(|b| b.is_ascii_digit()))
        .then_some(raw)
}

/// Longest sign-in code accepted from a link (Telegram codes are 5 to 6).
const MAX_LOGIN_CODE: usize = 8;

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
        InternalLink::BotStartInGroup {
            username,
            start_parameter,
            administrator_rights,
        } if !username.is_empty() => LinkRoute::Resolve(A::AddBot {
            domain: username.clone(),
            invite: Invite {
                // tdesktop: a link with rights asks for admin groups only.
                scope: if administrator_rights.is_some() {
                    Scope::GroupAdmin
                } else {
                    Scope::All
                },
                requested_rights: *administrator_rights,
                start_parameter: start_parameter.clone(),
            },
        }),
        InternalLink::BotAddToChannel {
            username,
            administrator_rights,
        } if !username.is_empty() => LinkRoute::Resolve(A::AddBot {
            domain: username.clone(),
            invite: Invite {
                scope: Scope::ChannelAdmin,
                requested_rights: *administrator_rights,
                start_parameter: String::new(),
            },
        }),
        InternalLink::BotStartInGroup { .. } | InternalLink::BotAddToChannel { .. } => {
            LinkRoute::Message("This bot link is broken.".into())
        }
        InternalLink::WebApp {
            username,
            short_name,
            start_parameter,
        } if !username.is_empty() && !short_name.is_empty() => {
            LinkRoute::Resolve(A::OpenWebAppLink {
                domain: username.clone(),
                short_name: short_name.clone(),
                start_parameter: start_parameter.clone(),
            })
        }
        InternalLink::WebApp { .. } => LinkRoute::Message("This app link is broken.".into()),
        InternalLink::MainWebApp {
            username,
            start_parameter,
        } if !username.is_empty() => LinkRoute::Resolve(A::OpenMainWebApp {
            domain: username.clone(),
            start_parameter: start_parameter.clone(),
        }),
        InternalLink::MainWebApp { .. } => LinkRoute::Message("This app link is broken.".into()),
        InternalLink::AttachmentMenuBot { username, url } if !username.is_empty() => {
            LinkRoute::Resolve(A::OpenAttachmentBot {
                domain: username.clone(),
                url: url.clone(),
            })
        }
        InternalLink::AttachmentMenuBot { .. } => {
            LinkRoute::Message("This bot link is broken.".into())
        }
        InternalLink::Game {
            username,
            game_short_name,
        } if !username.is_empty() && !game_short_name.is_empty() => {
            LinkRoute::Resolve(A::ShareGame {
                domain: username.clone(),
                game_short_name: game_short_name.clone(),
            })
        }
        InternalLink::Game { .. } => LinkRoute::Message("This game link is broken.".into()),
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
        InternalLink::ChatBoost { url } => {
            if url.is_empty() {
                LinkRoute::Message("This boost link is broken.".into())
            } else {
                LinkRoute::Resolve(A::BoostLink { url: url.clone() })
            }
        }
        InternalLink::PremiumGiftCode => LinkRoute::Message(format!("Gift codes {NOT_SUPPORTED}")),
        // Opens the group or channel, whose header has the join button.
        // Joining straight from a link would open the microphone unasked, so
        // the call itself is not joined here.
        InternalLink::VideoChat { username, .. } if !username.is_empty() => {
            LinkRoute::Resolve(A::OpenUsername {
                domain: username.clone(),
                start_param: None,
                post: None,
                story_id: None,
            })
        }
        InternalLink::VideoChat { live_stream, .. } => LinkRoute::Message(if *live_stream {
            format!("Joining a live stream from a link {NOT_SUPPORTED}")
        } else {
            format!("Joining a video chat from a link {NOT_SUPPORTED}")
        }),
        InternalLink::GroupCall => {
            LinkRoute::Message(format!("Joining a call from a link {NOT_SUPPORTED}"))
        }
        InternalLink::ChatFolderInvite { invite_link } => {
            if invite_link.is_empty() {
                LinkRoute::Message("This folder link is broken or has expired.".into())
            } else {
                LinkRoute::Ui(DeepLinkUi::FolderInvite {
                    link: invite_link.clone(),
                })
            }
        }
        InternalLink::Background { name } => {
            if name.is_empty() {
                LinkRoute::Message("This wallpaper link is broken.".into())
            } else {
                LinkRoute::Ui(DeepLinkUi::Background { name: name.clone() })
            }
        }
        // `addtheme` links install a desktop theme file (`.tdesktop-theme`),
        // which Quill's own light and dark themes do not use.
        InternalLink::Theme => LinkRoute::Message(
            "Desktop theme links install Telegram Desktop theme files, which Quill doesn't use. Pick colors in Appearance instead.".into(),
        ),
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
mod tests;

#[cfg(test)]
mod web_app_link_tests;
