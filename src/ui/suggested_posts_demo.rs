//! `ready-suggested-posts`: a channel's direct messages as a member
//! (injected, no live Telegram): the member's own offer, the channel's
//! counter-offer with Accept / Decline / Suggest Changes, and the suggest
//! bar up in the composer.
//!
//! `QUILL_DEMO_SUGGEST` opens a box over it: `price` (Suggest a Message),
//! `time`, `accept` (Accept Terms), `decline`, or `admin` (the price box as
//! the channel's admin, countering).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::demo::seed_ready_chats_session;
use super::screenshot_demo::{DemoSpec, register_demos};
use super::suggested_posts_state::{SuggestTarget, TimeTarget};
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::{ChatId, MessageId};
use quill::local_time::now_unix;
use quill::state::Session;
use quill::suggested_posts::{SuggestBar, SuggestMode, SuggestTerms, SuggestedPrice};
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

register_demos![
    DemoSpec::ready(
        "ready-suggested-posts",
        seed_suggested_posts_session,
        "screenshot demo — suggested posts: offer block, decision buttons, suggest bar (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_suggested_posts),
];

/// The channel's direct messages chat.
pub(super) const CHAT: i64 = -1_000_000_000_777;
const SUPERGROUP: i64 = 777;
/// The member's original offer.
pub(super) const OFFER: i64 = 101;
/// The channel's counter-offer, replying to it.
pub(super) const COUNTER: i64 = 102;

/// Midnight-free times for the fixtures: a day and two days ahead.
fn ahead(days: i64) -> i64 {
    let now = now_unix();
    now.saturating_add(days.saturating_mul(86_400))
        .saturating_sub(now.rem_euclid(3600))
}

pub(super) fn seed_suggested_posts_session(sink: Arc<MemorySink>) -> Session {
    seed_with_role(sink, false)
}

/// The demo chat; `admin` makes the viewer the channel's admin.
pub(super) fn seed_with_role(sink: Arc<MemorySink>, admin: bool) -> Session {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let mut session = seed_ready_chats_session(sink);
    let seq = AtomicU64::new(session.last_seq);
    let mut apply = |json: &str| {
        if let Some(owned) = copy_and_parse(json, &seq, &dyn_sink) {
            session.apply(owned);
        }
    };
    apply(&format!(
        r#"{{"@type":"updateNewChat","chat":{{"id":{CHAT},"title":"Quill Weekly","type":{{"@type":"chatTypeSupergroup","supergroup_id":{SUPERGROUP},"is_channel":false}},"unread_count":0}}}}"#
    ));
    apply(&format!(
        r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":{SUPERGROUP},"is_channel":false,"is_direct_messages_group":true,"is_administered_direct_messages_group":{admin},"status":{{"@type":"chatMemberStatusMember"}}}}}}"#
    ));
    apply(
        r#"{"@type":"updateOwnedStarCount","star_amount":{"@type":"starAmount","star_count":1200,"nanostar_count":0}}"#,
    );
    let pending = |price: &str, send_date: i64, can: bool| {
        format!(
            r#"{{"@type":"suggestedPostInfo","price":{price},"send_date":{send_date},"state":{{"@type":"suggestedPostStatePending"}},"can_be_approved":{can},"can_be_declined":{can}}}"#
        )
    };
    let text = |text: &str| {
        format!(
            r#"{{"@type":"messageText","text":{{"@type":"formattedText","text":"{text}","entities":[]}}}}"#
        )
    };
    apply(&format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":{OFFER},"chat_id":{CHAT},"is_outgoing":true,"date":{date},"content":{content},"suggested_post_info":{info}}}}}"#,
        date = now_unix().saturating_sub(7200),
        content = text(
            "Our spring release is out: faster sync, a new media viewer and message effects. Here is what changed."
        ),
        info = pending(
            r#"{"@type":"suggestedPostPriceStar","star_count":500}"#,
            0,
            false
        ),
    ));
    apply(&format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":{COUNTER},"chat_id":{CHAT},"is_outgoing":false,"sender_id":{{"@type":"messageSenderChat","chat_id":{CHAT}}},"date":{date},"reply_to":{{"@type":"messageReplyToMessage","chat_id":{CHAT},"message_id":{OFFER},"quote":null,"origin":null,"origin_send_date":0,"content":null}},"content":{content},"suggested_post_info":{info}}}}}"#,
        date = now_unix().saturating_sub(3600),
        content = text(
            "Our spring release is out: faster sync, a new media viewer and message effects. Here is what changed."
        ),
        info = pending(
            r#"{"@type":"suggestedPostPriceStar","star_count":750}"#,
            ahead(2),
            true
        ),
    ));
    session.open_chat(ChatId(CHAT));
    if !admin {
        session.payments.suggested_posts.bars.insert(
            CHAT,
            SuggestBar::new(SuggestTerms {
                price: Some(SuggestedPrice::Stars(250)),
                send_date: i32::try_from(ahead(1)).unwrap_or(0),
            }),
        );
    }
    session
}

impl QuillApp {
    fn demo_suggested_posts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let chat_id = ChatId(CHAT);
        let terms = self
            .session()
            .and_then(|session| session.suggest_bar(chat_id))
            .map(|bar| bar.terms)
            .unwrap_or_default();
        match std::env::var("QUILL_DEMO_SUGGEST")
            .unwrap_or_default()
            .as_str()
        {
            "price" => self.open_suggest_price(
                SuggestTarget::Bar(chat_id),
                SuggestMode::New,
                terms,
                false,
                window,
                cx,
            ),
            "admin" => {
                if let Some(session) = self.demo_session.as_mut() {
                    session.groups.supergroup_direct_messages.insert(
                        SUPERGROUP,
                        quill::suggested_posts::DirectMessagesRole::Admin,
                    );
                }
                self.suggest_edit_price(chat_id, MessageId(COUNTER), window, cx);
                if let Some(price) = self.suggested_posts.price.as_mut() {
                    price.admin = true;
                }
            }
            "time" => self.open_suggest_time(
                TimeTarget::PriceBox,
                SuggestMode::New,
                terms.send_date,
                window,
                cx,
            ),
            "accept" => self.begin_suggest_accept(chat_id, MessageId(COUNTER), window, cx),
            "decline" => self.open_suggest_decline(chat_id, MessageId(COUNTER), window, cx),
            _ => {}
        }
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "suggested_posts_tests.rs"]
mod tests;
