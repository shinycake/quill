//! Connect-driver tests: suggested posts in channel direct messages
//! (recorded TDLib JSON in, requests out).
use super::super::*;
use super::*;
use crate::composer::{DraftTopic, SendOptions};
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::{ChatId, MessageId};
use crate::platform::MemorySecretStore;
use crate::suggested_posts::{
    DirectMessagesRole, SuggestBar, SuggestTerms, SuggestedPostState, SuggestedPrice,
};
use crate::telegram::client::copy_and_parse;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

const CHAT: ChatId = ChatId(-1_000_000_000_077);

struct Fixture {
    driver: ConnectDriver<Arc<RecordingSender>>,
    recorder: Arc<RecordingSender>,
    dyn_sink: Arc<dyn DiagnosticSink>,
    seq: AtomicU64,
    dir: std::path::PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let dyn_sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
        let recorder = Arc::new(RecordingSender::new());
        let seq = AtomicU64::new(0);
        let driver = ready_driver(&recorder, prepared, &dyn_sink, &seq);
        Self {
            driver,
            recorder,
            dyn_sink,
            seq,
            dir,
        }
    }

    fn ingest(&mut self, json: &str) {
        self.driver
            .ingest(copy_and_parse(json, &self.seq, &self.dyn_sink).unwrap())
            .unwrap();
    }

    fn sent(&self, ty: &str) -> Vec<Value> {
        self.recorder
            .snapshot()
            .into_iter()
            .filter_map(|j| serde_json::from_str::<Value>(&j).ok())
            .filter(|v| v["@type"] == ty)
            .collect()
    }

    fn last(&self, ty: &str) -> Value {
        self.sent(ty).last().cloned().unwrap_or(Value::Null)
    }

    /// The direct messages group of a channel, as a member, opened.
    fn open_direct_messages(&mut self, administered: bool) {
        self.ingest(&format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{},"title":"News","type":{{"@type":"chatTypeSupergroup","supergroup_id":77,"is_channel":false}},"unread_count":0}}}}"#,
            CHAT.0
        ));
        self.ingest(&format!(
            r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":77,"is_channel":false,"is_direct_messages_group":true,"is_administered_direct_messages_group":{administered},"status":{{"@type":"chatMemberStatusMember"}}}}}}"#
        ));
        self.driver.select_chat(CHAT).unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn stars(count: i64, send_date: i32) -> SuggestTerms {
    SuggestTerms {
        price: Some(SuggestedPrice::Stars(count)),
        send_date,
    }
}

#[test]
fn supergroup_flags_give_the_role() {
    let mut f = Fixture::new();
    f.open_direct_messages(false);
    assert_eq!(
        f.driver.session.direct_messages_role(CHAT),
        Some(DirectMessagesRole::Member)
    );
    f.ingest(
        r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":77,"is_direct_messages_group":true,"is_administered_direct_messages_group":true,"status":{"@type":"chatMemberStatusMember"}}}"#,
    );
    assert_eq!(
        f.driver.session.direct_messages_role(CHAT),
        Some(DirectMessagesRole::Admin)
    );
    f.ingest(
        r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":77,"status":{"@type":"chatMemberStatusMember"}}}"#,
    );
    assert_eq!(f.driver.session.direct_messages_role(CHAT), None);
}

#[test]
fn approve_sends_the_date_and_blocks_a_second_tap() {
    let mut f = Fixture::new();
    f.open_direct_messages(true);
    f.driver
        .approve_suggested_post(CHAT, MessageId(5 << 20), 1_900_000_000)
        .unwrap();
    let sent = f.last("approveSuggestedPost");
    assert_eq!(sent["chat_id"], CHAT.0);
    assert_eq!(sent["message_id"], 5 << 20);
    assert_eq!(sent["send_date"], 1_900_000_000);
    assert!(
        f.driver
            .approve_suggested_post(CHAT, MessageId(5 << 20), 0)
            .is_err()
    );
    let extra = sent["@extra"].as_str().unwrap_or_default().to_string();
    f.ingest(&format!(r#"{{"@type":"ok","@extra":"{extra}"}}"#));
    f.driver
        .approve_suggested_post(CHAT, MessageId(5 << 20), 0)
        .unwrap();
    assert_eq!(f.last("approveSuggestedPost")["send_date"], 0);
}

#[test]
fn decline_sends_the_reason_and_an_error_becomes_a_note() {
    let mut f = Fixture::new();
    f.open_direct_messages(true);
    f.driver
        .decline_suggested_post(CHAT, MessageId(6 << 20), "  Not our topic ")
        .unwrap();
    let sent = f.last("declineSuggestedPost");
    assert_eq!(sent["comment"], "Not our topic");
    let extra = sent["@extra"].as_str().unwrap_or_default().to_string();
    f.ingest(&format!(
        r#"{{"@type":"error","@extra":"{extra}","code":400,"message":"SUGGESTED_POST_WRONG"}}"#
    ));
    let notes = f.driver.session.take_suggested_post_notes();
    assert_eq!(
        notes,
        vec!["Couldn't decline the post: invalid request.".to_string()]
    );
    assert!(f.driver.session.payments.suggested_posts.busy.is_empty());
}

#[test]
fn add_offer_carries_the_complete_terms() {
    let mut f = Fixture::new();
    f.open_direct_messages(false);
    f.driver
        .add_offer(CHAT, MessageId(7 << 20), stars(50, 1_900_000_000))
        .unwrap();
    let sent = f.last("addOffer");
    let info = &sent["options"]["suggested_post_info"];
    assert_eq!(info["@type"], "inputSuggestedPostInfo");
    assert_eq!(info["price"]["@type"], "suggestedPostPriceStar");
    assert_eq!(info["price"]["star_count"], 50);
    assert_eq!(info["send_date"], 1_900_000_000);
    f.driver
        .add_offer(
            CHAT,
            MessageId(7 << 20),
            SuggestTerms {
                price: Some(SuggestedPrice::TonCents(250)),
                send_date: 0,
            },
        )
        .unwrap();
    let info = f.last("addOffer")["options"]["suggested_post_info"].clone();
    assert_eq!(info["price"]["gram_cent_count"], 250);
    assert_eq!(info["send_date"], 0);
}

#[test]
fn changed_content_is_a_new_reply_and_media_is_copied() {
    let mut f = Fixture::new();
    f.open_direct_messages(true);
    f.driver
        .send_suggested_change(
            CHAT,
            MessageId(8 << 20),
            "New **text**",
            false,
            stars(10, 0),
        )
        .unwrap();
    let sent = f.last("sendMessage");
    assert_eq!(sent["reply_to"]["message_id"], 8 << 20);
    assert_eq!(sent["input_message_content"]["@type"], "inputMessageText");
    assert_eq!(sent["input_message_content"]["text"]["text"], "New text");
    assert_eq!(
        sent["options"]["suggested_post_info"]["price"]["star_count"],
        10
    );
    f.driver
        .send_suggested_change(CHAT, MessageId(8 << 20), "Caption", true, stars(10, 0))
        .unwrap();
    let content = f.last("sendMessage")["input_message_content"].clone();
    assert_eq!(content["@type"], "inputMessageForwarded");
    assert_eq!(content["copy_options"]["send_copy"], true);
    assert_eq!(content["copy_options"]["replace_caption"], true);
    assert_eq!(content["copy_options"]["new_caption"]["text"], "Caption");
}

#[test]
fn a_send_with_the_bar_is_a_suggested_post() {
    let options = SendOptions {
        suggested_post: Some(stars(25, 0)),
        ..SendOptions::default()
    };
    let json = crate::telegram::requests::send_text(
        crate::ids::RequestId(1),
        CHAT,
        None,
        "post me",
        None,
        &options,
    );
    let sent: Value = serde_json::from_str(&json).unwrap_or(Value::Null);
    assert_eq!(
        sent["options"]["suggested_post_info"]["price"]["star_count"],
        25
    );
    let plain = crate::telegram::requests::send_text(
        crate::ids::RequestId(2),
        CHAT,
        None,
        "plain",
        None,
        &SendOptions::default(),
    );
    let plain: Value = serde_json::from_str(&plain).unwrap_or(Value::Null);
    assert!(plain["options"]["suggested_post_info"].is_null());
}

#[test]
fn the_bar_rides_the_draft_both_ways() {
    let mut f = Fixture::new();
    f.open_direct_messages(false);
    let outcome = f
        .driver
        .set_suggest_bar(CHAT, Some(SuggestBar::new(stars(30, 0))), "", None, 1_000)
        .unwrap();
    assert_eq!(outcome, DraftSaveOutcome::Sent);
    let draft = f.last("setChatDraftMessage");
    assert_eq!(
        draft["draft_message"]["suggested_post_info"]["price"]["star_count"],
        30
    );
    let stored = f.driver.session.draft_for(CHAT, DraftTopic::Chat).cloned();
    assert_eq!(stored.and_then(|d| d.suggested_post), Some(stars(30, 0)));

    // Another client's draft with other terms moves the bar.
    f.ingest(&format!(
        r#"{{"@type":"updateChatDraftMessage","chat_id":{},"draft_message":{{"@type":"draftMessage","reply_to":null,"date":5,"content":{{"@type":"draftMessageContentText","text":{{"@type":"formattedText","text":"hi","entities":[]}},"link_preview_options":null}},"effect_id":"0","suggested_post_info":{{"@type":"inputSuggestedPostInfo","price":{{"@type":"suggestedPostPriceGram","gram_cent_count":120}},"send_date":0}}}},"positions":[]}}"#,
        CHAT.0
    ));
    assert_eq!(
        f.driver
            .session
            .suggest_bar(CHAT)
            .map(|bar| bar.terms.price),
        Some(Some(SuggestedPrice::TonCents(120)))
    );

    // Closing the bar clears the suggestion from the draft.
    f.driver
        .set_suggest_bar(CHAT, None, "hi", None, 2_000)
        .unwrap();
    let draft = f.last("setChatDraftMessage");
    assert!(draft["draft_message"]["suggested_post_info"].is_null());
    assert_eq!(f.driver.session.suggest_bar(CHAT), None);
}

#[test]
fn info_updates_change_the_loaded_message() {
    let mut f = Fixture::new();
    f.open_direct_messages(true);
    f.ingest(&format!(
        r#"{{"@type":"updateNewMessage","message":{{"@type":"message","id":{id},"chat_id":{chat},"sender_id":{{"@type":"messageSenderUser","user_id":5}},"is_outgoing":false,"date":1700000000,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"Post me","entities":[]}}}},"suggested_post_info":{{"@type":"suggestedPostInfo","price":null,"send_date":0,"state":{{"@type":"suggestedPostStatePending"}},"can_be_approved":true,"can_be_declined":true}}}}}}"#,
        id = 9 << 20,
        chat = CHAT.0
    ));
    let state = |f: &Fixture| {
        f.driver
            .session
            .histories
            .get(&CHAT.0)
            .and_then(|h| h.messages.get(&(9 << 20)))
            .and_then(|m| m.extras.suggested_post)
            .map(|info| info.state)
    };
    assert_eq!(state(&f), Some(SuggestedPostState::Pending));
    f.ingest(&format!(
        r#"{{"@type":"updateMessageSuggestedPostInfo","chat_id":{},"message_id":{},"suggested_post_info":{{"@type":"suggestedPostInfo","price":null,"send_date":0,"state":{{"@type":"suggestedPostStateApproved"}},"can_be_approved":false,"can_be_declined":false}}}}"#,
        CHAT.0,
        9 << 20
    ));
    assert_eq!(state(&f), Some(SuggestedPostState::Approved));
}
