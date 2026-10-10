//! Connect driver: `t.me` / `tg:` deep-link handling
//! (`parity:platform-deep-links`).
//!
//! Flow: the UI hands a launch link to [`ConnectDriver::request_deep_link_info`]
//! once auth is Ready; the `deepLinkInfo` answer is reduced into
//! `Session::deep_link` (see `session_apply.rs`); the UI then calls
//! [`ConnectDriver::resolve_deep_link`] for the parsed action, and the
//! follow-up answer becomes `DeepLinkState::ChatReady`, which the UI
//! consumes to open the chat.
use super::*;
use crate::ids::{ChatId, RequestId};
use crate::state::ChatsPurpose;
use crate::state::{DeepLinkAction, DeepLinkState, RequestPurpose};
use crate::telegram::requests::{
    check_chat_invite_link, create_private_chat, get_chat, get_chat_boost_link_info,
    get_deep_link_info, get_internal_link_type, get_message_link_info, join_chat_by_invite_link,
    search_public_chat, search_sticker_set_by_name, search_user_by_phone_number,
};
use crate::text::{TextEntity, TextEntityKind};

/// The first `textEntityTypeTextUrl` entity whose URL is a `tg://` link —
/// TDLib's marker for the resolved deep-link action.
pub fn deep_link_tg_url(entities: &[TextEntity]) -> Option<&str> {
    entities.iter().find_map(|entity| match &entity.kind {
        TextEntityKind::TextUrl { url } if url.starts_with("tg://") => Some(url.as_str()),
        _ => None,
    })
}

/// Tiny percent-decoder for `tg://` query values (usernames are ASCII,
/// but `start=` params may carry escapes). Malformed escapes are kept
/// verbatim rather than failing the whole link.
fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(hi), Some(lo)) = (
                (bytes[i + 1] as char).to_digit(16),
                (bytes[i + 2] as char).to_digit(16),
            )
        {
            out.push((hi * 16 + lo) as u8);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| value.to_string())
}

/// Query params of a `tg://` URL as `(name, decoded value)` pairs.
pub(crate) fn tg_query_params(url: &str) -> Vec<(String, String)> {
    let query = url.split('?').nth(1).unwrap_or("");
    query
        .split('&')
        .filter_map(|pair| {
            let (name, value) = pair.split_once('=')?;
            Some((name.to_string(), percent_decode(value)))
        })
        .collect()
}

/// Parse a `tg://` URL into a [`DeepLinkAction`]. Documented forms
/// (core.telegram.org/api/links): `tg://resolve?domain=` (with optional
/// `start=`, `post=`, `story=`), `tg://join?invite=`,
/// `tg://openmessage?user_id=&message_id=`,
/// `tg://privatepost?channel=&post=`, `tg://user?id=`. Anything else
/// (`tg://proxy`, unknown hosts) is `None` and the UI falls back to
/// TDLib's info text.
pub fn parse_tg_url(url: &str) -> Option<DeepLinkAction> {
    let rest = url.strip_prefix("tg://")?;
    let host = rest.split('?').next().unwrap_or("");
    let params = tg_query_params(url);
    let param = |name: &str| {
        params
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.clone())
    };
    match host {
        "resolve" => {
            let domain = param("domain").filter(|d| !d.is_empty())?;
            Some(DeepLinkAction::OpenUsername {
                domain,
                start_param: param("start").filter(|s| !s.is_empty()),
                post: param("post").and_then(|p| p.parse().ok()),
                story_id: param("story").and_then(|s| s.parse().ok()),
            })
        }
        "join" => {
            let hash = param("invite").filter(|h| !h.is_empty())?;
            Some(DeepLinkAction::JoinInvite { hash })
        }
        "openmessage" => Some(DeepLinkAction::OpenMessage {
            user_id: param("user_id")?.parse().ok()?,
            message_id: param("message_id")?.parse().ok()?,
        }),
        "privatepost" => Some(DeepLinkAction::OpenChannelPost {
            channel_id: param("channel")?.parse().ok()?,
            post: param("post")?.parse().ok()?,
        }),
        "user" => Some(DeepLinkAction::OpenUser {
            user_id: param("id")?.parse().ok()?,
        }),
        _ => None,
    }
}

/// First path segments of `t.me` links that are not usernames (stickers,
/// proxies, languages, share sheets, ...). They fall through to TDLib's
/// own `getDeepLinkInfo` text instead of a bogus `searchPublicChat`.
const WEB_RESERVED_PATHS: [&str; 24] = [
    "addstickers",
    "addemoji",
    "addtheme",
    "addlist",
    "proxy",
    "socks",
    "setlanguage",
    "share",
    "msg",
    "login",
    "confirmphone",
    "bg",
    "invoice",
    "boost",
    "giftcode",
    "m",
    "s",
    "iv",
    "contact",
    "joinchat",
    "c",
    "+",
    "web",
    "k",
];

fn is_username(segment: &str) -> bool {
    !segment.is_empty()
        && segment
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// `https://t.me/...` (also `telegram.me`, `telegram.dog`) → action, the
/// web spellings of the `tg://` forms: `t.me/<user>[/<post>][?start=]`,
/// `t.me/<user>/s/<story>`, `t.me/+<hash>`, `t.me/joinchat/<hash>`,
/// `t.me/c/<channel>/<post>`. Anything else is `None`.
pub fn parse_web_url(url: &str) -> Option<DeepLinkAction> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let rest = rest.split('#').next().unwrap_or(rest);
    let (host, tail) = rest.split_once('/')?;
    if !["t.me", "telegram.me", "telegram.dog"]
        .iter()
        .any(|h| host.eq_ignore_ascii_case(h))
    {
        return None;
    }
    let (path, query) = tail.split_once('?').unwrap_or((tail, ""));
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let first = *segments.first()?;
    if let Some(hash) = first.strip_prefix('+') {
        // `t.me/+15550001`: all digits is a phone number, not an invite
        // (TDLib resolves it as `internalLinkTypeUserPhoneNumber`).
        if hash.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        return Some(DeepLinkAction::JoinInvite { hash: hash.into() });
    }
    match first {
        "joinchat" => {
            return segments.get(1).map(|hash| DeepLinkAction::JoinInvite {
                hash: (*hash).into(),
            });
        }
        "c" => {
            let channel_id = segments.get(1)?.parse().ok()?;
            let post = segments.iter().skip(2).rev().find_map(|s| s.parse().ok())?;
            return Some(DeepLinkAction::OpenChannelPost { channel_id, post });
        }
        _ => {}
    }
    if !is_username(first) || WEB_RESERVED_PATHS.contains(&first.to_ascii_lowercase().as_str()) {
        return None;
    }
    let params = tg_query_params(&format!("x?{query}"));
    let start_param = params
        .iter()
        .find(|(n, _)| n == "start")
        .map(|(_, v)| v.clone())
        .filter(|v| !v.is_empty());
    let (post, story_id) = match (segments.get(1), segments.get(2)) {
        (Some(&"s"), Some(story)) => (None, story.parse().ok()),
        (Some(_), _) => (
            segments.iter().skip(1).rev().find_map(|s| s.parse().ok()),
            None,
        ),
        _ => (None, None),
    };
    Some(DeepLinkAction::OpenUsername {
        domain: first.into(),
        start_param,
        post,
        story_id,
    })
}

/// Any Telegram link Quill can resolve locally (`tg://` or web form).
pub fn parse_deep_link_url(url: &str) -> Option<DeepLinkAction> {
    parse_tg_url(url).or_else(|| parse_web_url(url))
}

/// Entity list → action, via the first `tg://` TextUrl entity.
pub fn parse_deep_link_action(entities: &[TextEntity]) -> Option<DeepLinkAction> {
    deep_link_tg_url(entities).and_then(parse_tg_url)
}

/// First CLI arg that is a Telegram deep link. Flags (`--*`) are skipped;
/// validation is `deep_link_inbox::sanitize_link` (the same gate applied to
/// links forwarded by a second launch or handed over by the OS).
pub fn detect_deep_link_arg(args: &[String]) -> Option<String> {
    args.iter()
        .skip(1)
        .filter(|arg| !arg.starts_with("--"))
        .find_map(|arg| crate::deep_link_inbox::sanitize_link(arg))
}

impl<S: JsonSender> ConnectDriver<S> {
    /// `parity:platform-deep-links`: `getDeepLinkInfo` for a launch link.
    /// Generation-guarded like `resolve_inline_bot`; a flow already in
    /// progress wins (one launch link at a time).
    pub fn request_deep_link_info(
        &mut self,
        link: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.deep_link.is_some() {
            return Ok(None);
        }
        // Like tdesktop's `openLocalUrl`, route the link by its shape
        // locally. `getDeepLinkInfo` only knows server-side deep links
        // (it answers 404 for `resolve?domain=` and `t.me/<user>`).
        // Forms with parameters the local parsers drop (`?comment=`,
        // `?thread=`, `?t=`, `?startgroup`, ...) and everything else go to
        // TDLib's `getInternalLinkType`.
        if !crate::deep_link_types::needs_internal_resolution(link)
            && let Some(action) = parse_deep_link_url(link)
        {
            return self.resolve_deep_link(action);
        }
        self.send_deep_link_lookup(link, true)
    }

    /// `getDeepLinkInfo` for a link TDLib called unknown (tdesktop
    /// `HandleUnknown`): the answer is the explanation shown to the user.
    pub fn request_deep_link_text(
        &mut self,
        link: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.send_deep_link_lookup(link, false)
    }

    /// One lookup in the `ResolvingInfo` slot: `getInternalLinkType`
    /// (`internal`) or `getDeepLinkInfo`.
    fn send_deep_link_lookup(
        &mut self,
        link: &str,
        internal: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        self.session.deep_link_seq = self.session.deep_link_seq.wrapping_add(1);
        let generation = self.session.deep_link_seq;
        self.session.deep_link = Some(DeepLinkState::ResolvingInfo { generation });
        let (purpose, build): (_, fn(RequestId, &str) -> String) = if internal {
            (
                RequestPurpose::Chats(ChatsPurpose::DeepLinkInternalType { generation }),
                get_internal_link_type,
            )
        } else {
            (
                RequestPurpose::Chats(ChatsPurpose::DeepLinkInfo { generation }),
                get_deep_link_info,
            )
        };
        if internal {
            self.session.deep_link_original = link.to_string();
        }
        let extra = self.session.request(purpose, None);
        let json = build(extra, link);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.deep_link =
                    Some(DeepLinkState::ShowText("Couldn't reach Telegram.".into()));
                Err(err)
            }
        }
    }

    /// Join only the currently displayed, checked invite generation.
    pub fn confirm_deep_link_invite(
        &mut self,
        generation: u64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(DeepLinkState::InvitePreview {
            hash,
            generation: slot,
            ..
        }) = self.session.deep_link.clone()
        else {
            return Ok(None);
        };
        if slot != generation {
            return Ok(None);
        }
        let extra = self.session.request(
            RequestPurpose::Chats(ChatsPurpose::DeepLinkJoin { generation }),
            None,
        );
        let json = join_chat_by_invite_link(extra, &format!("https://t.me/+{hash}"));
        self.session.deep_link = Some(DeepLinkState::ResolvingChat {
            action: DeepLinkAction::JoinInvite { hash },
            generation,
        });
        match self.sender.send_json(&json) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.deep_link =
                    Some(DeepLinkState::ShowText("Couldn't reach Telegram.".into()));
                Err(err)
            }
        }
    }

    /// `parity:platform-deep-links`: fire the follow-up request for a
    /// parsed deep-link action — `searchPublicChat` for a username (or the
    /// story author's), `createPrivateChat` for `openmessage` / `user`,
    /// `getChat` for `privatepost`, `checkChatInviteLink` for an invite
    /// hash. The answer becomes `DeepLinkState::ChatReady` in
    /// `Session::apply`.
    pub fn resolve_deep_link(
        &mut self,
        action: DeepLinkAction,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        // A share draft waits for the user to pick a chat: nothing to resolve.
        if !self.chats_path_active() || matches!(action, DeepLinkAction::ShareDraft { .. }) {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.deep_link_seq = self.session.deep_link_seq.wrapping_add(1);
        let generation = self.session.deep_link_seq;
        let purpose = match &action {
            DeepLinkAction::JoinInvite { .. } => {
                RequestPurpose::Chats(ChatsPurpose::DeepLinkCheckInvite { generation })
            }
            _ => RequestPurpose::Chats(ChatsPurpose::DeepLinkResolve { generation }),
        };
        let extra = self.session.request(purpose, None);
        let json = match &action {
            DeepLinkAction::OpenUsername { domain, .. }
            | DeepLinkAction::OpenPublicChatDraft { domain, .. }
            | DeepLinkAction::ShareGame { domain, .. }
            | DeepLinkAction::AddBot { domain, .. }
            | DeepLinkAction::OpenWebAppLink { domain, .. }
            | DeepLinkAction::OpenMainWebApp { domain, .. }
            | DeepLinkAction::OpenAttachmentBot { domain, .. } => search_public_chat(extra, domain),
            DeepLinkAction::MessageLink { url } => get_message_link_info(extra, url),
            DeepLinkAction::BoostLink { url } => get_chat_boost_link_info(extra, url),
            DeepLinkAction::OpenChannelBoost { chat_id } => get_chat(extra, ChatId(*chat_id)),
            DeepLinkAction::StickerSet { name } => search_sticker_set_by_name(extra, name),
            DeepLinkAction::UserPhone { phone, .. } => search_user_by_phone_number(extra, phone),
            DeepLinkAction::OpenChatById { chat_id, .. } => get_chat(extra, ChatId(*chat_id)),
            // Refused above; kept total so a new action cannot panic here.
            DeepLinkAction::ShareDraft { .. } => search_public_chat(extra, ""),
            DeepLinkAction::JoinInvite { hash } => {
                check_chat_invite_link(extra, &format!("https://t.me/+{hash}"))
            }
            DeepLinkAction::OpenMessage { user_id, .. }
            | DeepLinkAction::OpenUser { user_id }
            | DeepLinkAction::OpenUserDraft { user_id, .. } => {
                create_private_chat(extra, *user_id, false)
            }
            DeepLinkAction::OpenChannelPost { channel_id, .. } => {
                // TDLib channel dialog encoding: -(10^12) - channel_id.
                get_chat(extra, ChatId(-1_000_000_000_000 - channel_id))
            }
        };
        self.session.deep_link = Some(DeepLinkState::ResolvingChat { action, generation });
        match self.sender.send_json(&json) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.deep_link =
                    Some(DeepLinkState::ShowText("Couldn't reach Telegram.".into()));
                Err(err)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::TextEntityKind;

    fn tg_entity(url: &str) -> TextEntity {
        TextEntity {
            utf8_start: 0,
            utf8_end: 4,
            kind: TextEntityKind::TextUrl {
                url: url.to_string(),
            },
        }
    }

    #[test]
    fn parse_resolve_variants() {
        assert_eq!(
            parse_tg_url("tg://resolve?domain=durov"),
            Some(DeepLinkAction::OpenUsername {
                domain: "durov".into(),
                start_param: None,
                post: None,
                story_id: None,
            })
        );
        assert_eq!(
            parse_tg_url("tg://resolve?domain=BotFather&start=hello%20world&post=42&story=7"),
            Some(DeepLinkAction::OpenUsername {
                domain: "BotFather".into(),
                start_param: Some("hello world".into()),
                post: Some(42),
                story_id: Some(7),
            })
        );
        // Missing domain is not actionable.
        assert_eq!(parse_tg_url("tg://resolve?start=x"), None);
    }

    #[test]
    fn parse_join_and_openmessage() {
        assert_eq!(
            parse_tg_url("tg://join?invite=AbCdEfGhIjKlMnOp"),
            Some(DeepLinkAction::JoinInvite {
                hash: "AbCdEfGhIjKlMnOp".into()
            })
        );
        assert_eq!(
            parse_tg_url("tg://openmessage?user_id=123&message_id=456"),
            Some(DeepLinkAction::OpenMessage {
                user_id: 123,
                message_id: 456
            })
        );
        // Missing params are not actionable.
        assert_eq!(parse_tg_url("tg://openmessage?user_id=123"), None);
        assert_eq!(parse_tg_url("tg://join?invite="), None);
    }

    #[test]
    fn parse_privatepost_and_user() {
        assert_eq!(
            parse_tg_url("tg://privatepost?channel=123&post=456"),
            Some(DeepLinkAction::OpenChannelPost {
                channel_id: 123,
                post: 456
            })
        );
        assert_eq!(
            parse_tg_url("tg://user?id=987"),
            Some(DeepLinkAction::OpenUser { user_id: 987 })
        );
        // Missing params are not actionable.
        assert_eq!(parse_tg_url("tg://privatepost?channel=123"), None);
        assert_eq!(parse_tg_url("tg://user"), None);
    }

    #[test]
    fn parse_web_links() {
        let user = |domain: &str, start: Option<&str>, post, story| {
            Some(DeepLinkAction::OpenUsername {
                domain: domain.into(),
                start_param: start.map(Into::into),
                post,
                story_id: story,
            })
        };
        assert_eq!(
            parse_deep_link_url("https://t.me/durov"),
            user("durov", None, None, None)
        );
        assert_eq!(
            parse_deep_link_url("https://telegram.me/durov/42?single"),
            user("durov", None, Some(42), None)
        );
        assert_eq!(
            parse_deep_link_url("https://t.me/BotFather?start=a%20b#x"),
            user("BotFather", Some("a b"), None, None)
        );
        assert_eq!(
            parse_deep_link_url("https://t.me/durov/s/7"),
            user("durov", None, None, Some(7))
        );
        assert_eq!(
            parse_deep_link_url("https://t.me/+AbCdEf"),
            Some(DeepLinkAction::JoinInvite {
                hash: "AbCdEf".into()
            })
        );
        assert_eq!(
            parse_deep_link_url("https://t.me/joinchat/AbCdEf"),
            Some(DeepLinkAction::JoinInvite {
                hash: "AbCdEf".into()
            })
        );
        assert_eq!(
            parse_deep_link_url("https://t.me/c/123/456"),
            Some(DeepLinkAction::OpenChannelPost {
                channel_id: 123,
                post: 456
            })
        );
        assert_eq!(parse_deep_link_url("https://t.me/addstickers/Pack"), None);
        // Phone links are TDLib's to resolve, not invite hashes.
        assert_eq!(parse_deep_link_url("https://t.me/+15550001"), None);
        assert_eq!(parse_deep_link_url("https://t.me/"), None);
        assert_eq!(parse_deep_link_url("https://evil.com/durov"), None);
    }

    #[test]
    fn parse_unknown_hosts_are_none() {
        assert_eq!(parse_tg_url("tg://proxy?server=x&port=1"), None);
        assert_eq!(parse_tg_url("tg://settings"), None);
        assert_eq!(parse_tg_url("https://t.me/durov"), None);
    }

    #[test]
    fn entity_extraction_picks_first_tg_url() {
        let entities = vec![
            tg_entity("https://example.com"),
            tg_entity("tg://resolve?domain=durov"),
        ];
        assert_eq!(
            parse_deep_link_action(&entities),
            Some(DeepLinkAction::OpenUsername {
                domain: "durov".into(),
                start_param: None,
                post: None,
                story_id: None,
            })
        );
        assert_eq!(parse_deep_link_action(&[]), None);
    }

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn detects_link_schemes_and_skips_flags() {
        assert_eq!(
            detect_deep_link_arg(&args(&["quill", "tg://resolve?domain=durov"])),
            Some("tg://resolve?domain=durov".into())
        );
        assert_eq!(
            detect_deep_link_arg(&args(&["quill", "--connect-smoke", "https://t.me/durov"])),
            Some("https://t.me/durov".into())
        );
        assert_eq!(
            detect_deep_link_arg(&args(&["quill", "http://telegram.me/durov/42"])),
            Some("http://telegram.me/durov/42".into())
        );
        assert_eq!(
            detect_deep_link_arg(&args(&["quill", "--connect-smoke"])),
            None
        );
        assert_eq!(detect_deep_link_arg(&args(&["quill"])), None);
        // Not a link arg.
        assert_eq!(detect_deep_link_arg(&args(&["quill", "notes.txt"])), None);
    }
}
