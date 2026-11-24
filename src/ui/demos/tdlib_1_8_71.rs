//! Screenshot demos for what TDLib 1.8.71 added (docs/decisions/tdlib-1-8-71.md),
//! all injected, no live Telegram:
//!
//! - `ready-bot-invite`: a chat with a bot someone invited the user to: the
//!   bot-invite bar (Block bot, Report Spam, who invited and who owns it)
//!   over the bot's `messageBotInvite` line.
//! - `ready-bot-invite-box`: the "Invite to Bot" box for a bot the user owns,
//!   two people ticked.
//! - `ready-bot-invite-profile`: that bot's info panel with "Invite to Bot".
//! - `ready-gift-on-chain`: on-chain gift service messages (an incoming
//!   on-chain offer, the user's own accepted one, a gift bought on-chain),
//!   text only.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::demo::seed_ready_chats_session;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::ChatId;
use quill::state::Session;
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

register_demos![
    DemoSpec::ready(
        "ready-bot-invite",
        seed_bot_invite,
        "screenshot demo — bot invite bar and service message (TDLib 1.8.71)"
    ),
    DemoSpec::ready(
        "ready-bot-invite-box",
        seed_owned_bot,
        "screenshot demo — Invite to Bot box (TDLib 1.8.71)"
    )
    .setup(QuillApp::demo_invite_to_bot_box),
    DemoSpec::ready(
        "ready-bot-invite-profile",
        seed_owned_bot,
        "screenshot demo — an owned bot's profile with Invite to Bot (TDLib 1.8.71)"
    )
    .setup(QuillApp::demo_owned_bot_profile),
    DemoSpec::ready(
        "ready-gift-on-chain",
        seed_gift_on_chain,
        "screenshot demo — on-chain gift offers and origins (TDLib 1.8.71)"
    ),
];

const BOT: i64 = 4_100;
const INVITER: i64 = 41;
const OWNER: i64 = 42;
const PEOPLE: [(i64, &str, &str); 4] = [
    (41, "Dana", "Levi"),
    (42, "Omer", "Katz"),
    (43, "Maya", "Cohen"),
    (44, "Noa", "Mizrahi"),
];

fn bot_user(id: i64, name: &str, owned: bool) -> String {
    format!(
        r#"{{"@type":"updateUser","user":{{"id":{id},"first_name":"{name}","last_name":"","usernames":{{"@type":"usernames","active_usernames":["helper_demo_bot"],"disabled_usernames":[],"editable_username":"helper_demo_bot","collectible_usernames":[]}},"type":{{"@type":"userTypeBot","can_be_edited":{owned},"can_join_groups":true,"can_read_all_group_messages":false,"has_main_web_app":false,"has_topics":false,"allows_users_to_create_topics":false,"can_manage_bots":false,"is_inline":false,"inline_query_placeholder":"","supports_guest_queries":false,"is_guard":false,"need_location":false,"can_connect_to_business":false,"can_be_added_to_attachment_menu":false,"active_user_count":0}}}}}}"#
    )
}

fn person(id: i64, first: &str, last: &str) -> String {
    format!(
        r#"{{"@type":"updateUser","user":{{"id":{id},"first_name":"{first}","last_name":"{last}","type":{{"@type":"userTypeRegular"}},"is_contact":true}}}}"#
    )
}

fn private_chat(id: i64, title: &str, order: i64, action_bar: &str) -> [String; 2] {
    [
        format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{id},"title":"{title}","type":{{"@type":"chatTypePrivate","user_id":{id}}},"unread_count":0,"can_be_reported":true,"action_bar":{action_bar}}}}}"#
        ),
        format!(
            r#"{{"@type":"updateChatPosition","chat_id":{id},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"{order}","is_pinned":false}}}}"#
        ),
    ]
}

fn message(id: i64, chat: i64, outgoing: bool, sender: i64, content: &str) -> String {
    format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{chat},"is_outgoing":{outgoing},"sender_id":{{"@type":"messageSenderUser","user_id":{sender}}},"date":1790632000,"content":{content}}}}}"#
    )
}

fn apply_all(session: &mut Session, sink: Arc<MemorySink>, jsons: &[String]) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink;
    let seq = AtomicU64::new(session.last_seq);
    for json in jsons {
        if let Some(owned) = copy_and_parse(json, &seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

/// The people and their chats, after the seed's own chats.
fn people() -> Vec<String> {
    let mut jsons = Vec::new();
    for (index, (id, first, last)) in PEOPLE.iter().enumerate() {
        jsons.push(person(*id, first, last));
        let order = 50 - i64::try_from(index).unwrap_or(0);
        jsons.extend(private_chat(*id, &format!("{first} {last}"), order, "null"));
    }
    jsons
}

fn seed_bot_invite(sink: Arc<MemorySink>) -> Session {
    let mut session = seed_ready_chats_session(sink.clone());
    let mut jsons = people();
    jsons.push(bot_user(BOT, "Helper Bot", false));
    jsons.extend(private_chat(
        BOT,
        "Helper Bot",
        60,
        &format!(
            r#"{{"@type":"chatActionBarBotInvite","can_unarchive":false,"inviter_user_id":{INVITER},"owner_user_id":{OWNER}}}"#
        ),
    ));
    jsons.push(message(
        900,
        BOT,
        false,
        BOT,
        &format!(r#"{{"@type":"messageBotInvite","inviter_user_id":{INVITER}}}"#),
    ));
    jsons.push(message(
        901,
        BOT,
        false,
        BOT,
        r#"{"@type":"messageText","text":{"@type":"formattedText","text":"Hi! Dana thought you might like me. Send /start to begin.","entities":[]}}"#,
    ));
    apply_all(&mut session, sink, &jsons);
    session.open_chat(ChatId(BOT));
    session
}

fn seed_owned_bot(sink: Arc<MemorySink>) -> Session {
    let mut session = seed_ready_chats_session(sink.clone());
    let mut jsons = people();
    jsons.push(bot_user(BOT, "Helper Bot", true));
    jsons.extend(private_chat(BOT, "Helper Bot", 60, "null"));
    apply_all(&mut session, sink, &jsons);
    session.open_chat(ChatId(BOT));
    session
}

fn gift(name: &str, number: i32) -> String {
    format!(
        r#"{{"@type":"upgradedGift","id":"77","regular_gift_id":"5","title":"{name}","name":"{name}-{number}","number":{number},"total_upgraded_count":100,"max_upgraded_count":1000}}"#
    )
}

fn seed_gift_on_chain(sink: Arc<MemorySink>) -> Session {
    let mut session = seed_ready_chats_session(sink.clone());
    let mut jsons = people();
    // 12.5 and 40 Grams, in their smallest units.
    let incoming = format!(
        r#"{{"@type":"messageUpgradedGiftOnChainPurchaseOffer","gift":{gift},"is_buyer":false,"state":{{"@type":"giftOnChainPurchaseOfferStatePending"}},"offer_address":"EQ-demo","gram_amount":"12500000000","fee_basis_points":500,"expiration_date":1790718400,"offer_message_id":0}}"#,
        gift = gift("PlushPepe", 1204),
    );
    let mine = format!(
        r#"{{"@type":"messageUpgradedGiftOnChainPurchaseOffer","gift":{gift},"is_buyer":true,"state":{{"@type":"giftOnChainPurchaseOfferStatePending"}},"offer_address":"EQ-demo2","gram_amount":"40000000000","fee_basis_points":500,"expiration_date":1790718400,"offer_message_id":0}}"#,
        gift = gift("SnoopDogg", 77),
    );
    let accepted = format!(
        r#"{{"@type":"messageUpgradedGiftOnChainPurchaseOffer","gift":{gift},"is_buyer":true,"state":{{"@type":"giftOnChainPurchaseOfferStateAccepted"}},"offer_address":"EQ-demo3","gram_amount":"40000000000","fee_basis_points":500,"expiration_date":1790718400,"offer_message_id":0}}"#,
        gift = gift("SnoopDogg", 78),
    );
    let declined = format!(
        r#"{{"@type":"messageUpgradedGiftOnChainPurchaseOffer","gift":{gift},"is_buyer":false,"state":{{"@type":"giftOnChainPurchaseOfferStateDeclined","refund_amount":"0"}},"offer_address":"EQ-demo4","gram_amount":"9000000000","fee_basis_points":500,"expiration_date":1790718400,"offer_message_id":0}}"#,
        gift = gift("PlushPepe", 1205),
    );
    jsons.push(message(910, 43, false, 43, &incoming));
    jsons.push(message(911, 43, true, 1, &mine));
    jsons.push(message(912, 43, true, 1, &accepted));
    jsons.push(message(913, 43, false, 43, &declined));
    apply_all(&mut session, sink, &jsons);
    session.open_chat(ChatId(43));
    session
}

impl QuillApp {
    fn demo_owned_bot_profile(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_info_panel_target(quill::state::InfoPanelTarget::User(BOT), window, cx);
        cx.notify();
    }

    fn demo_invite_to_bot_box(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if self.open_invite_to_bot(BOT, cx)
            && let Some(invite) = self.dialogs.invite_to_bot.as_mut()
        {
            invite.toggle(43);
            invite.toggle(41);
        }
        cx.notify();
    }
}
