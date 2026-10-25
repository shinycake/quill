use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::{AccountKey, ChatId, MessageId};
use crate::platform::MemorySecretStore;
use crate::premium_hub::PremiumFeature;
use crate::state::{ReactionChoice, Session};
use crate::telegram::client::copy_and_parse;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

const CHAT: ChatId = ChatId(7);
const MESSAGE: MessageId = MessageId(50);

struct Harness {
    driver: ConnectDriver<Arc<RecordingSender>>,
    recorder: Arc<RecordingSender>,
    seq: AtomicU64,
    sink: Arc<dyn DiagnosticSink>,
    _dir: std::path::PathBuf,
}

impl Harness {
    fn new() -> Self {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), sink.clone());
        let driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let mut h = Harness {
            driver,
            recorder,
            seq: AtomicU64::new(0),
            sink,
            _dir: dir,
        };
        h.ingest(r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#);
        h.ingest(r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#);
        h.ingest(r#"{"@type":"updateChatPosition","chat_id":7,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"9","is_pinned":false}}"#);
        h.ingest(r#"{"@type":"updateNewMessage","message":{"@type":"message","id":50,"chat_id":7,"is_outgoing":false,"date":1700000000,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}}}"#);
        h.ingest(&reactions_update(&[("👍", 2, true), ("❤", 1, false)]));
        h
    }

    fn ingest(&mut self, json: &str) {
        self.driver
            .ingest(copy_and_parse(json, &self.seq, &self.sink).unwrap())
            .unwrap();
    }

    fn sent(&self, ty: &str) -> Vec<Value> {
        self.recorder
            .snapshot()
            .into_iter()
            .filter(|j| j.contains(&format!("\"@type\":\"{ty}\"")))
            .map(|j| serde_json::from_str(&j).unwrap())
            .collect()
    }

    fn open_menu(&mut self) {
        self.driver
            .fetch_message_reactions(CHAT, MESSAGE)
            .unwrap()
            .expect("request");
        let config = self.sent("getApplicationConfig");
        assert_eq!(config.len(), 1, "the app config is asked for once");
        self.ingest(&format!(
            r#"{{"@type":"jsonValueObject","@extra":{},"members":[
              {{"@type":"jsonObjectMember","key":"reactions_user_max_default","value":{{"@type":"jsonValueNumber","value":1}}}},
              {{"@type":"jsonObjectMember","key":"reactions_user_max_premium","value":{{"@type":"jsonValueNumber","value":3}}}}]}}"#,
            config[0]["@extra"]
        ));
        let options = &self.sent("getMessageAvailableReactions")[0];
        self.ingest(&format!(
            r#"{{"@type":"availableReactions","@extra":{},"top_reactions":[{{"@type":"availableReaction","type":{{"@type":"reactionTypeEmoji","emoji":"❤"}},"needs_premium":false}},{{"@type":"availableReaction","type":{{"@type":"reactionTypeCustomEmoji","custom_emoji_id":"77"}},"needs_premium":true}}],"recent_reactions":[],"popular_reactions":[],"allow_custom_emoji":true,"are_tags":false,"unavailability_reason":null}}"#,
            options["@extra"]
        ));
    }

    fn reactions(&self) -> Vec<(String, i32, bool)> {
        self.driver
            .session
            .histories
            .get(&CHAT.0)
            .and_then(|history| history.messages.get(&MESSAGE.0))
            .and_then(|message| message.interaction_info.as_ref())
            .and_then(|info| info.reactions.as_ref())
            .map(|reactions| {
                reactions
                    .reactions
                    .iter()
                    .map(|one| {
                        (
                            one.reaction_type.emoji_text().unwrap_or("?").to_string(),
                            one.total_count,
                            one.is_chosen,
                        )
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

fn reactions_update(list: &[(&str, i32, bool)]) -> String {
    let items: Vec<String> = list
        .iter()
        .map(|(emoji, count, chosen)| {
            format!(
                r#"{{"@type":"messageReaction","type":{{"@type":"reactionTypeEmoji","emoji":"{emoji}"}},"total_count":{count},"is_chosen":{chosen},"used_sender_id":null,"recent_sender_ids":[]}}"#
            )
        })
        .collect();
    format!(
        r#"{{"@type":"updateMessageInteractionInfo","chat_id":7,"message_id":50,"interaction_info":{{"@type":"messageInteractionInfo","view_count":0,"forward_count":0,"reply_info":null,"reactions":{{"@type":"messageReactions","reactions":[{}],"are_tags":false,"paid_reactors":[],"can_get_added_reactions":false}}}}}}"#,
        items.join(",")
    )
}

#[test]
fn a_free_account_swaps_its_reaction_before_tdlib_answers() {
    let mut h = Harness::new();
    h.open_menu();
    assert_eq!(h.driver.session.reaction_limit(), Some(1));
    h.driver
        .toggle_reaction_choice(CHAT, MESSAGE, &ReactionChoice::Emoji("❤".into()))
        .unwrap();
    assert_eq!(h.sent("addMessageReaction").len(), 1);
    assert_eq!(
        h.reactions(),
        vec![("👍".into(), 1, false), ("❤".into(), 2, true)],
        "the earlier reaction is taken back at once"
    );
    h.ingest(&reactions_update(&[("❤", 2, true), ("👍", 1, false)]));
    assert_eq!(
        h.reactions(),
        vec![("❤".into(), 2, true), ("👍".into(), 1, false)]
    );
}

#[test]
fn premium_keeps_up_to_its_limit_and_the_quick_reaction_trims_too() {
    let mut h = Harness::new();
    h.driver.session.payments.premium_option = Some(true);
    h.open_menu();
    assert_eq!(h.driver.session.reaction_limit(), Some(3));
    h.driver
        .toggle_reaction_choice(CHAT, MESSAGE, &ReactionChoice::Emoji("❤".into()))
        .unwrap();
    h.driver
        .toggle_message_reaction(CHAT, MESSAGE, "🔥")
        .unwrap();
    assert_eq!(
        h.reactions(),
        vec![
            ("👍".into(), 2, true),
            ("❤".into(), 2, true),
            ("🔥".into(), 1, true)
        ]
    );
    h.driver
        .toggle_message_reaction(CHAT, MESSAGE, "🎉")
        .unwrap();
    assert_eq!(
        h.reactions(),
        vec![
            ("👍".into(), 2, true),
            ("❤".into(), 2, true),
            ("🎉".into(), 1, true)
        ],
        "the third own reaction makes room for the new one"
    );
    assert_eq!(h.sent("addMessageReaction").len(), 3);
}

#[test]
fn without_the_app_config_nothing_is_trimmed() {
    let mut h = Harness::new();
    assert_eq!(h.driver.session.reaction_limit(), None);
    h.driver
        .toggle_message_reaction(CHAT, MESSAGE, "❤")
        .unwrap();
    assert_eq!(
        h.reactions(),
        vec![("👍".into(), 2, true), ("❤".into(), 2, true)]
    );
}

#[test]
fn reactions_tdlib_marks_premium_need_premium_on_a_free_account() {
    let mut h = Harness::new();
    h.open_menu();
    let session = &h.driver.session;
    let options = session
        .stickers
        .message_reaction_options
        .ready_for(CHAT, MESSAGE)
        .expect("options");
    assert_eq!(
        options.premium_locked,
        vec![ReactionChoice::CustomEmoji(77)]
    );
    assert_eq!(
        session.react_premium_feature(CHAT, MESSAGE, &ReactionChoice::CustomEmoji(77)),
        Some(PremiumFeature::UniqueReactions)
    );
    assert_eq!(
        session.react_premium_feature(CHAT, MESSAGE, &ReactionChoice::CustomEmoji(5)),
        Some(PremiumFeature::UniqueReactions),
        "any custom emoji outside a channel"
    );
    assert_eq!(
        session.react_premium_feature(CHAT, MESSAGE, &ReactionChoice::Emoji("❤".into())),
        None
    );
    h.driver.session.payments.premium_option = Some(true);
    assert_eq!(
        h.driver
            .session
            .react_premium_feature(CHAT, MESSAGE, &ReactionChoice::CustomEmoji(77)),
        None
    );
}

#[test]
fn a_custom_reaction_already_on_the_message_or_in_a_channel_is_free() {
    let mut h = Harness::new();
    h.ingest(r#"{"@type":"updateMessageInteractionInfo","chat_id":7,"message_id":50,"interaction_info":{"@type":"messageInteractionInfo","view_count":0,"forward_count":0,"reply_info":null,"reactions":{"@type":"messageReactions","reactions":[{"@type":"messageReaction","type":{"@type":"reactionTypeCustomEmoji","custom_emoji_id":"77"},"total_count":4,"is_chosen":false,"used_sender_id":null,"recent_sender_ids":[]}],"are_tags":false,"paid_reactors":[],"can_get_added_reactions":false}}}"#);
    assert_eq!(
        h.driver
            .session
            .react_premium_feature(CHAT, MESSAGE, &ReactionChoice::CustomEmoji(77)),
        None
    );
    h.ingest(r#"{"@type":"updateNewChat","chat":{"id":-1009,"title":"News","type":{"@type":"chatTypeSupergroup","supergroup_id":9,"is_channel":true},"unread_count":0}}"#);
    h.ingest(r#"{"@type":"updateNewMessage","message":{"@type":"message","id":60,"chat_id":-1009,"is_outgoing":false,"is_channel_post":true,"date":1700000000,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"post","entities":[]}}}}"#);
    assert_eq!(
        h.driver.session.react_premium_feature(
            ChatId(-1009),
            MessageId(60),
            &ReactionChoice::CustomEmoji(5)
        ),
        None
    );
}

#[test]
fn saved_messages_tags_need_premium() {
    let mut h = Harness::new();
    h.ingest(r#"{"@type":"updateMessageInteractionInfo","chat_id":7,"message_id":50,"interaction_info":{"@type":"messageInteractionInfo","view_count":0,"forward_count":0,"reply_info":null,"reactions":{"@type":"messageReactions","reactions":[{"@type":"messageReaction","type":{"@type":"reactionTypeEmoji","emoji":"👍"},"total_count":1,"is_chosen":true,"used_sender_id":null,"recent_sender_ids":[]}],"are_tags":true,"paid_reactors":[],"can_get_added_reactions":false}}}"#);
    assert_eq!(
        h.driver
            .session
            .react_premium_feature(CHAT, MESSAGE, &ReactionChoice::Emoji("👍".into())),
        Some(PremiumFeature::SavedMessagesTags)
    );
}
