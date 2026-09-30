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
use crate::state::{DeepLinkAction, DeepLinkState, RequestPurpose};
use crate::telegram::requests::{
    create_private_chat, get_chat, get_deep_link_info, join_chat_by_invite_link, search_public_chat,
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
fn tg_query_params(url: &str) -> Vec<(String, String)> {
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

/// Entity list → action, via the first `tg://` TextUrl entity.
pub fn parse_deep_link_action(entities: &[TextEntity]) -> Option<DeepLinkAction> {
    deep_link_tg_url(entities).and_then(parse_tg_url)
}

/// First CLI arg that looks like a Telegram deep link. Flags (`--*`)
/// are skipped; matches `tg://`, `t.me` and `telegram.me` http(s) links.
/// Used by `main.rs` to stash the launch link on the app.
pub fn detect_deep_link_arg(args: &[String]) -> Option<String> {
    args.iter()
        .skip(1)
        .find(|arg| {
            !arg.starts_with("--")
                && (arg.starts_with("tg://")
                    || arg.starts_with("https://t.me/")
                    || arg.starts_with("http://t.me/")
                    || arg.starts_with("https://telegram.me/")
                    || arg.starts_with("http://telegram.me/"))
        })
        .cloned()
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
        self.session.deep_link_seq = self.session.deep_link_seq.wrapping_add(1);
        let generation = self.session.deep_link_seq;
        self.session.deep_link = Some(DeepLinkState::ResolvingInfo { generation });
        let extra = self
            .session
            .request(RequestPurpose::DeepLinkInfo { generation }, None);
        let json = get_deep_link_info(extra, link);
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
    /// `getChat` for `privatepost`, `joinChatByInviteLink` for an invite
    /// hash. The answer becomes `DeepLinkState::ChatReady` in
    /// `Session::apply`.
    pub fn resolve_deep_link(
        &mut self,
        action: DeepLinkAction,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.deep_link_seq = self.session.deep_link_seq.wrapping_add(1);
        let generation = self.session.deep_link_seq;
        let purpose = match &action {
            DeepLinkAction::JoinInvite { .. } => RequestPurpose::DeepLinkJoin { generation },
            _ => RequestPurpose::DeepLinkResolve { generation },
        };
        let extra = self.session.request(purpose, None);
        let json = match &action {
            DeepLinkAction::OpenUsername { domain, .. } => search_public_chat(extra, domain),
            DeepLinkAction::JoinInvite { hash } => {
                join_chat_by_invite_link(extra, &format!("https://t.me/+{hash}"))
            }
            DeepLinkAction::OpenMessage { user_id, .. } | DeepLinkAction::OpenUser { user_id } => {
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
