//! Chats TDLib shows in the main list as a promotion (`chatPosition.source`):
//! a proxy sponsor or a public service announcement. tdesktop paints a badge
//! where the date goes (`dialogs_layout.cpp`, `PaintRowTopRight`), the
//! announcement text as the preview, and offers "Hide this announcement" for
//! announcements only (`addHidePromotion`). Whether a promotion applies to a
//! chat at all is `History::useTopPromotion`, [`top_promotion`] here.
#![deny(clippy::indexing_slicing)]

use crate::lang::{PackValue, keys, tr};
use crate::state::{ChatSummary, Session};
use crate::telegram::envelope::{ChannelMemberStatus, ChatKind};
use serde_json::Value;
use std::collections::HashMap;

/// Prefix of the server-provided label of an announcement type.
pub const PSA_LABEL_PREFIX: &str = "cloud_lng_badge_psa_";
/// Prefix of the server-provided "about this announcement" text of a type
/// (tdesktop `kPsaAboutPrefix`).
pub const PSA_ABOUT_PREFIX: &str = "cloud_lng_about_psa_";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Promotion {
    /// `chatSourceMtprotoProxy`: the proxy server sponsors the chat.
    MtprotoProxy,
    /// `chatSourcePublicServiceAnnouncement`; `kind` is its `type` (empty
    /// when TDLib sent none, which tdesktop cannot hide either) and `text`
    /// the announcement.
    Psa { kind: String, text: String },
}

impl Promotion {
    /// The promotion a non-null `chatPosition.source` describes. An
    /// unknown source is shown like an announcement without a type.
    pub fn from_source(source: &Value) -> Self {
        match source.get("@type").and_then(Value::as_str) {
            Some("chatSourceMtprotoProxy") => Self::MtprotoProxy,
            _ => {
                let field = |name: &str| {
                    source
                        .get(name)
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string()
                };
                Self::Psa {
                    kind: field("type"),
                    text: field("text"),
                }
            }
        }
    }

    /// The announcement type, when there is one.
    pub fn psa_kind(&self) -> Option<&str> {
        match self {
            Self::Psa { kind, .. } if !kind.is_empty() => Some(kind),
            _ => None,
        }
    }

    /// The announcement text, which replaces the row's preview.
    pub fn preview(&self) -> Option<&str> {
        match self {
            Self::Psa { text, .. } if !text.is_empty() => Some(text),
            _ => None,
        }
    }

    /// tdesktop offers "Hide this announcement" only for a typed
    /// announcement; a proxy sponsor goes away with the proxy.
    pub fn can_hide(&self) -> bool {
        self.psa_kind().is_some()
    }
}

/// `History::useTopPromotion`: the chat has a source and is not pinned, and
/// is a channel the account is not in, or a bot with an empty history.
pub fn top_promotion<'a>(session: &'a Session, chat: &ChatSummary) -> Option<&'a Promotion> {
    let promotion = session.chat_list.promoted_chats.get(&chat.id.0)?;
    if chat.is_pinned {
        return None;
    }
    let applies = match chat.kind {
        ChatKind::Supergroup { .. } => !matches!(
            chat.my_member_status,
            Some(
                ChannelMemberStatus::Creator
                    | ChannelMemberStatus::Administrator
                    | ChannelMemberStatus::Member
                    | ChannelMemberStatus::Restricted
            )
        ),
        ChatKind::Private { user_id } => {
            session.user(user_id.0).is_some_and(|user| user.is_bot)
                && chat.last_message.is_none()
                && chat.last_preview.is_empty()
        }
        _ => false,
    };
    applies.then_some(promotion)
}

/// What a promoted row shows besides the chat itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowBadge {
    /// The text where the date goes.
    pub label: String,
    /// The announcement text, shown as the preview.
    pub preview: Option<String>,
}

/// How many times one type's text is asked for in one pack. A request
/// that failed while connected may be asked again; past this the generic
/// text stays until the language changes.
const PSA_ASK_ATTEMPTS: u8 = 3;

/// Where one announcement type's text stands in the current pack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PsaAsk {
    /// Out under `request`; its answer settles it, its error or a send
    /// that did not leave returns it to asking again (while `attempts`
    /// last).
    Asking {
        request: crate::ids::RequestId,
        attempts: u8,
    },
    /// Answered (the server may have no text for the type).
    Ready,
    /// The last ask failed after `attempts` tries.
    Failed { attempts: u8 },
}

/// What one pump asks for: the language-pack keys of the types still
/// wanted, tagged with the pack's epoch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PsaPlan {
    pub epoch: u32,
    pub keys: Vec<String>,
    kinds: Vec<String>,
}

/// The server's badge text per announcement type for one language pack,
/// and where asking for each type stands.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PsaLabels {
    pack: String,
    /// Changes with the pack, so an answer for an older pack is ignored.
    epoch: u32,
    cloud: HashMap<String, String>,
    /// The server's "about" text per type.
    about: HashMap<String, String>,
    asks: HashMap<String, PsaAsk>,
}

impl PsaLabels {
    /// The row badge: "Proxy sponsor", the server's text for the type, or
    /// "PSA".
    pub fn row(&self, promotion: &Promotion) -> RowBadge {
        let label = match promotion {
            Promotion::MtprotoProxy => tr(keys::PROXY_SPONSOR).to_string(),
            Promotion::Psa { kind, .. } => self
                .cloud
                .get(kind)
                .cloned()
                .unwrap_or_else(|| tr(keys::BADGE_PSA_DEFAULT).to_string()),
        };
        RowBadge {
            label,
            preview: promotion.preview().map(str::to_string),
        }
    }

    /// The language-pack keys still to fetch for `kinds` in `pack`. A new
    /// pack forgets what the old one gave. Nothing is marked asked here:
    /// [`Self::begin`] does that once the request exists.
    pub fn plan<'a>(&mut self, pack: &str, kinds: impl IntoIterator<Item = &'a str>) -> PsaPlan {
        if self.pack != pack {
            self.pack = pack.to_string();
            self.epoch = self.epoch.wrapping_add(1);
            self.cloud.clear();
            self.about.clear();
            self.asks.clear();
        }
        let mut wanted: Vec<String> = Vec::new();
        for kind in kinds {
            let open = match self.asks.get(kind) {
                None => true,
                Some(PsaAsk::Failed { attempts }) => *attempts < PSA_ASK_ATTEMPTS,
                Some(PsaAsk::Asking { .. } | PsaAsk::Ready) => false,
            };
            if open && !wanted.iter().any(|k| k == kind) {
                wanted.push(kind.to_string());
            }
        }
        let keys = wanted
            .iter()
            .flat_map(|kind| {
                [
                    format!("{PSA_LABEL_PREFIX}{kind}"),
                    format!("{PSA_ABOUT_PREFIX}{kind}"),
                ]
            })
            .collect();
        PsaPlan {
            epoch: self.epoch,
            keys,
            kinds: wanted,
        }
    }

    /// The plan's request exists: its types wait for its answer.
    pub fn begin(&mut self, plan: &PsaPlan, request: crate::ids::RequestId) {
        if plan.epoch != self.epoch {
            return;
        }
        for kind in &plan.kinds {
            let tries = match self.asks.get(kind) {
                Some(PsaAsk::Failed { attempts }) => *attempts,
                _ => 0,
            };
            self.asks.insert(
                kind.clone(),
                PsaAsk::Asking {
                    request,
                    attempts: tries.saturating_add(1),
                },
            );
        }
    }

    /// `request` failed (its error answer, or its send): the types that
    /// waited for it are failed, to be asked again while attempts last.
    pub fn fail_request(&mut self, request: crate::ids::RequestId) {
        for ask in self.asks.values_mut() {
            if let PsaAsk::Asking {
                request: waiting,
                attempts,
            } = *ask
                && waiting == request
            {
                *ask = PsaAsk::Failed { attempts };
            }
        }
    }

    /// Keep the labels a `languagePackStrings` answer of `request` carries,
    /// if it is for the pack asked about last.
    pub fn store(
        &mut self,
        epoch: u32,
        request: Option<crate::ids::RequestId>,
        strings: Vec<(String, PackValue)>,
    ) {
        if epoch != self.epoch {
            return;
        }
        for ask in self.asks.values_mut() {
            if matches!(*ask, PsaAsk::Asking { request: waiting, .. } if Some(waiting) == request) {
                *ask = PsaAsk::Ready;
            }
        }
        for (key, value) in strings {
            let PackValue::Ordinary(text) = value else {
                continue;
            };
            if text.is_empty() {
                continue;
            }
            if let Some(kind) = key.strip_prefix(PSA_LABEL_PREFIX) {
                self.cloud.insert(kind.to_string(), text);
            } else if let Some(kind) = key.strip_prefix(PSA_ABOUT_PREFIX) {
                self.about.insert(kind.to_string(), text);
            }
        }
    }

    /// The text of the one-time box (`showAboutTopPromotion`): the proxy
    /// sponsor text when there is no type, else the server's text for the
    /// type, else the default announcement text. `**bold**` marks bold.
    pub fn about(&self, promotion: &Promotion) -> String {
        match promotion.psa_kind() {
            None => tr(keys::PROXY_SPONSOR_ABOUT).to_string(),
            Some(kind) => self
                .about
                .get(kind)
                .cloned()
                .unwrap_or_else(|| tr(keys::ABOUT_PSA_DEFAULT).to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A pump that asked: the plan and its request (ids count up).
    fn ask<'a>(
        labels: &mut PsaLabels,
        pack: &str,
        kinds: impl IntoIterator<Item = &'a str>,
    ) -> (u32, crate::ids::RequestId, Vec<String>) {
        let plan = labels.plan(pack, kinds);
        let id = crate::ids::RequestId(labels.asks.len() as u64 + u64::from(plan.epoch) * 100);
        labels.begin(&plan, id);
        (plan.epoch, id, plan.keys)
    }

    fn psa(kind: &str, text: &str) -> Promotion {
        Promotion::Psa {
            kind: kind.into(),
            text: text.into(),
        }
    }

    #[test]
    fn source_types() {
        let proxy = Promotion::from_source(&json!({"@type": "chatSourceMtprotoProxy"}));
        assert_eq!(proxy, Promotion::MtprotoProxy);
        assert!(!proxy.can_hide());
        let typed = Promotion::from_source(&json!({
            "@type": "chatSourcePublicServiceAnnouncement",
            "type": "covid",
            "text": "Stay home",
        }));
        assert_eq!(typed, psa("covid", "Stay home"));
        assert_eq!(typed.psa_kind(), Some("covid"));
        assert_eq!(typed.preview(), Some("Stay home"));
        assert!(typed.can_hide());
        let untyped = Promotion::from_source(&json!({
            "@type": "chatSourcePublicServiceAnnouncement",
        }));
        assert!(!untyped.can_hide());
        assert_eq!(untyped.preview(), None);
    }

    #[test]
    fn labels_prefer_the_servers_text() {
        let mut labels = PsaLabels::default();
        let covid = psa("covid", "");
        assert_eq!(labels.row(&covid).label, "PSA");
        assert_eq!(labels.row(&Promotion::MtprotoProxy).label, "Proxy sponsor");
        let (epoch, id, _) = ask(&mut labels, "en", ["covid"]);
        labels.store(
            epoch,
            Some(id),
            vec![
                (
                    "cloud_lng_badge_psa_covid".into(),
                    PackValue::Ordinary("COVID-19 Info".into()),
                ),
                ("lng_other".into(), PackValue::Ordinary("x".into())),
                (
                    "cloud_lng_badge_psa_empty".into(),
                    PackValue::Ordinary(String::new()),
                ),
            ],
        );
        assert_eq!(labels.row(&covid).label, "COVID-19 Info");
        assert_eq!(labels.row(&psa("empty", "")).label, "PSA");
        let with_text = labels.row(&psa("covid", "Wash your hands"));
        assert_eq!(with_text.preview.as_deref(), Some("Wash your hands"));
    }

    #[test]
    fn each_type_is_asked_for_once_per_pack() {
        let mut labels = PsaLabels::default();
        let (_, _, keys) = ask(&mut labels, "en", ["covid", "covid", "war"]);
        assert_eq!(
            keys,
            vec![
                "cloud_lng_badge_psa_covid",
                "cloud_lng_about_psa_covid",
                "cloud_lng_badge_psa_war",
                "cloud_lng_about_psa_war"
            ]
        );
        assert!(labels.plan("en", ["covid", "war"]).keys.is_empty());
        let (_, _, keys) = ask(&mut labels, "ru", ["covid"]);
        assert_eq!(
            keys,
            vec!["cloud_lng_badge_psa_covid", "cloud_lng_about_psa_covid"]
        );
    }

    #[test]
    fn the_box_text_follows_the_type() {
        let mut labels = PsaLabels::default();
        let covid = psa("covid", "");
        let proxy = tr(keys::PROXY_SPONSOR_ABOUT);
        let default = tr(keys::ABOUT_PSA_DEFAULT);
        assert_eq!(labels.about(&Promotion::MtprotoProxy), proxy);
        assert_eq!(labels.about(&psa("", "text")), proxy, "no type");
        assert_eq!(labels.about(&covid), default, "unknown to the server");
        let (epoch, id, _) = ask(&mut labels, "en", ["covid", "empty"]);
        labels.store(
            epoch,
            Some(id),
            vec![
                (
                    "cloud_lng_about_psa_covid".into(),
                    PackValue::Ordinary("Stay **safe**".into()),
                ),
                (
                    "cloud_lng_about_psa_empty".into(),
                    PackValue::Ordinary(String::new()),
                ),
            ],
        );
        assert_eq!(labels.about(&covid), "Stay **safe**");
        assert_eq!(labels.about(&psa("empty", "")), default, "empty falls back");
        assert_eq!(labels.about(&psa("other", "")), default);
        labels.plan("ru", ["covid"]);
        assert_eq!(labels.about(&covid), default, "a new pack forgets it");
    }

    fn session() -> Session {
        let sink: std::sync::Arc<dyn crate::diagnostics::DiagnosticSink> =
            std::sync::Arc::new(crate::diagnostics::MemorySink::new());
        Session::new(crate::ids::AccountKey::primary(), sink)
    }

    fn put(session: &mut Session, id: i64, kind: ChatKind) {
        let mut chat = crate::state::placeholder_chat(crate::ids::ChatId(id));
        chat.kind = kind;
        session.chats.insert(id, chat);
        session
            .chat_list
            .promoted_chats
            .insert(id, Promotion::MtprotoProxy);
    }

    fn applies(session: &Session, id: i64) -> bool {
        session
            .chats
            .get(&id)
            .is_some_and(|chat| top_promotion(session, chat).is_some())
    }

    #[test]
    fn a_promotion_applies_like_use_top_promotion() {
        let mut s = session();
        let channel = ChatKind::Supergroup {
            supergroup_id: 1,
            is_channel: true,
        };
        put(&mut s, 1, channel);
        assert!(applies(&s, 1), "a channel the account is not in");
        if let Some(chat) = s.chats.get_mut(&1) {
            chat.my_member_status = Some(ChannelMemberStatus::Member);
        }
        assert!(!applies(&s, 1), "a joined channel");
        if let Some(chat) = s.chats.get_mut(&1) {
            chat.my_member_status = Some(ChannelMemberStatus::Left);
            chat.is_pinned = true;
        }
        assert!(!applies(&s, 1), "a pinned chat");
        put(&mut s, 2, ChatKind::BasicGroup { basic_group_id: 2 });
        assert!(!applies(&s, 2), "a basic group");
        put(
            &mut s,
            3,
            ChatKind::Private {
                user_id: crate::ids::UserId(3),
            },
        );
        assert!(!applies(&s, 3), "a user that is not a bot");
        let json = r#"{"@type":"updateUser","user":{"id":3,"first_name":"Bot","type":{"@type":"userTypeBot"}}}"#;
        let seq = std::sync::atomic::AtomicU64::new(0);
        let sink: std::sync::Arc<dyn crate::diagnostics::DiagnosticSink> =
            std::sync::Arc::new(crate::diagnostics::MemorySink::new());
        if let Some(owned) = crate::telegram::client::copy_and_parse(json, &seq, &sink) {
            s.apply(owned);
        }
        assert!(applies(&s, 3), "a bot with an empty history");
        if let Some(chat) = s.chats.get_mut(&3) {
            chat.last_preview = "hello".into();
        }
        assert!(!applies(&s, 3), "a bot with messages");
    }

    #[test]
    fn a_new_pack_drops_the_old_text_and_late_answers() {
        let mut labels = PsaLabels::default();
        let covid = psa("covid", "");
        let (old, old_id, _) = ask(&mut labels, "en", ["covid"]);
        let answer = || {
            vec![(
                "cloud_lng_badge_psa_covid".to_string(),
                PackValue::Ordinary("COVID-19 Info".into()),
            )]
        };
        labels.store(old, Some(old_id), answer());
        assert_eq!(labels.row(&covid).label, "COVID-19 Info");
        let (new, new_id, _) = ask(&mut labels, "ru", ["covid"]);
        assert_eq!(labels.row(&covid).label, "PSA");
        // The English answer arrives late: ignored.
        labels.store(old, Some(old_id), answer());
        assert_eq!(labels.row(&covid).label, "PSA");
        labels.store(new, Some(new_id), answer());
        assert_eq!(labels.row(&covid).label, "COVID-19 Info");
    }

    /// 04-071: a request that failed (or never left) is asked again, a
    /// few times; the answer settles it for good.
    #[test]
    fn a_failed_ask_is_asked_again_a_few_times() {
        use crate::ids::RequestId;
        let mut labels = PsaLabels::default();
        for attempt in 1..=PSA_ASK_ATTEMPTS {
            let plan = labels.plan("en", ["covid"]);
            assert_eq!(plan.keys.len(), 2, "attempt {attempt}");
            let id = RequestId(u64::from(attempt));
            labels.begin(&plan, id);
            // Out and unanswered: nothing asks beside it.
            assert!(labels.plan("en", ["covid"]).keys.is_empty());
            // Another request's error leaves it waiting.
            labels.fail_request(RequestId(99));
            assert!(labels.plan("en", ["covid"]).keys.is_empty());
            labels.fail_request(id);
        }
        assert!(labels.plan("en", ["covid"]).keys.is_empty(), "gave up");
        // A new pack starts over, and an answer settles the type.
        let plan = labels.plan("ru", ["covid"]);
        assert_eq!(plan.keys.len(), 2);
        labels.begin(&plan, RequestId(50));
        labels.store(plan.epoch, Some(RequestId(50)), Vec::new());
        assert!(labels.plan("ru", ["covid"]).keys.is_empty());
        assert_eq!(labels.row(&psa("covid", "")).label, "PSA");
    }
}
